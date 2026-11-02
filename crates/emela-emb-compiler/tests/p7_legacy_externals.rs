//! Exact old source bodies and C observations with explicit C roots.
mod support;
use emela_emb_compiler::{
    ExternalAdapter, ExternalResult, Options, Output, Source, compile_sources,
};

fn source(id: &str, exports: &[&str]) -> String {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("compat/legacy-corpus/manifest.json")).unwrap();
    let program = manifest["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == format!("program:{id}"))
        .unwrap();
    assert_eq!(program["files"].as_array().unwrap().len(), 1);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compat/legacy-corpus")
        .join(program["files"][0]["fixture"].as_str().unwrap());
    let mut source = std::fs::read_to_string(path).unwrap();
    for name in exports {
        let declaration = format!("pub fn {name}(");
        assert_eq!(source.matches(&declaration).count(), 1);
        source = source.replace(
            &declaration,
            &format!("@export(c,\"{name}\")\n{declaration}"),
        );
    }
    source
}
fn fixture(id: &str, exports: &[&str], options: &Options) -> Output {
    let source = source(id, exports);
    compile_sources(&[Source::new("main.emel", &source)], "main.emel", options)
        .unwrap_or_else(|e| panic!("{id}: {e:#?}"))
}
fn compile_and_run(output: Output, harness: &str) {
    support::compile_and_run(&output, harness);
}
#[test]
fn source_handler_can_call_native_external_int64_and_float_with_mixed_arguments() {
    let mut output = fixture(
        "7e82f10b73354f97dfeb",
        &["read"],
        &Options::default().with_heap_bytes(1024),
    );
    add_heap_check(&mut output);
    compile_and_run(
        output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { (void)k;(void)m;(void)n;abort(); }
double board_mix(int64_t a,double b) { assert(a==INT64_MIN);return b; }
int test_heap_empty(void);
int main(void) { for(int i=0;i<1000;i++) {
  const uint64_t expected=UINT64_C(0xfff8000000000042);uint64_t actual;double n;
  memcpy(&n,&expected,sizeof(n));double result=read(INT64_MIN,n);
  memcpy(&actual,&result,sizeof(actual));assert(actual==expected);assert(test_heap_empty());
}}
"#,
    );
}
#[test]
fn source_string_adapters_snapshot_borrowed_results_and_board_held_arguments() {
    let mut output = fixture(
        "092ecc4b39a71f6fe329",
        &["direct", "indirect", "discard"],
        &Options::default()
            .with_heap_bytes(1024)
            .with_external_adapter(
                "host_echo",
                ExternalAdapter::new("board_echo", ExternalResult::Borrowed),
            ),
    );
    add_heap_check(&mut output);
    compile_and_run(
        output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);static EmelaString held;
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) { (void)k;(void)m;(void)n;abort(); }
EmelaString board_echo(EmelaString value) { held=value;emela_external_own_4(&held);return value; }
int main(void) { for(int i=0;i<1000;i++) {
 char bytes[]={'a','\0','b'};EmelaString input={.data=bytes,.length=3};
 EmelaString value=direct(input);memset(bytes,'x',3);
 assert(value.owner&&value.length==3&&!memcmp(value.data,"a\0b",3));assert(held.owner&&!memcmp(held.data,"a\0b",3));
 emela_embedded_release_string(&value);emela_external_release_4(&held);assert(test_heap_empty());
 value=indirect(input);emela_embedded_release_string(&value);emela_external_release_4(&held);assert(test_heap_empty());
 discard(input);emela_external_release_4(&held);assert(test_heap_empty());
} }
"#,
    );
}

#[test]
fn source_tuple_and_list_external_values_keep_all_managed_fields_alive() {
    let mut output = fixture(
        "729d56d2f2d29fef4139",
        &["roundtrip", "text"],
        &Options::default()
            .with_heap_bytes(1024)
            .with_external_adapter(
                "host_packet",
                ExternalAdapter::new("board_packet", ExternalResult::Borrowed),
            ),
    );
    add_heap_check(&mut output);
    let header = &output
        .files
        .iter()
        .find(|f| f.path == "emela_embedded.h")
        .unwrap()
        .contents;
    let cty = header
        .lines()
        .find(|line| line.contains(" board_packet("))
        .unwrap()
        .split(" board_packet(")
        .next()
        .unwrap();
    let release = header
        .lines()
        .find(|line| {
            line.starts_with("void emela_external_release_")
                && line.contains(&format!("({cty} *value)"))
        })
        .unwrap()
        .strip_prefix("void ")
        .unwrap()
        .split('(')
        .next()
        .unwrap();
    compile_and_run(
        output.clone(),
        &format!(
            r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {{(void)k;(void)m;(void)n;abort();}}
{cty} board_packet({cty} packet) {{return packet;}}
int main(void) {{for(int i=0;i<1000;i++) {{char input[]={{'a','\0','b'}};{cty} packet=roundtrip((EmelaString){{.data=input,.length=3}});memset(input,'x',3);
 EmelaString value=text(packet);{release}(&packet);assert(value.owner&&value.length==3&&!memcmp(value.data,"a\0b",3));emela_embedded_release_string(&value);assert(test_heap_empty());}}}}
"#
        ),
    );
}

fn output() -> Output {
    let mut output = fixture(
        "efd3c3bdb37863da1c8f",
        &["exchange", "echoed", "scoped", "deferred", "seed"],
        &Options::default()
            .with_heap_bytes(4096)
            .with_fiber_capacity(8)
            .with_frame_capacity(24)
            .with_frame_bytes(1024)
            .with_deferred_capacity(2)
            .with_task_capacity(2)
            .with_event_capacity(2)
            .with_string_capacity(64)
            .with_task_adapter("host_value", "board_value_start")
            .with_task_adapter("host_packet", "board_packet_start")
            .with_task_adapter("host_text", "board_text_start"),
    );
    add_heap_check(&mut output);
    output
}
fn add_heap_check(output: &mut Output) {
    if !output.c_source.contains("emela_heap_storage") {
        output
            .files
            .iter_mut()
            .find(|f| f.path == "emela_embedded.c")
            .unwrap()
            .contents
            .push_str("\nint test_heap_empty(void){return 1;}\n");
        return;
    }
    output
        .files
        .iter_mut()
        .find(|f| f.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(
            r#"
#ifdef EMELA_UNWIND_RUNTIME
void test_cancel(EmelaFiber f) { emela_fiber_get(f)->cancelled=1; }
#endif
int test_heap_empty(void) {
  if (!emela_heap_initialized) return 1;
  for (unsigned char *p=emela_heap_storage;p<emela_heap_end();) {
    emela_heap_block *b=(emela_heap_block *)(void*)p;
    if (!b->is_free) { return 0; }
    p+=b->size;
  }
  return 1;
}
"#,
        );
}

fn packet_ids(output: &Output) -> (usize, usize) {
    let header = &output
        .files
        .iter()
        .find(|f| f.path == "emela_embedded.h")
        .unwrap()
        .contents;
    let ty: usize = header
        .lines()
        .find(|l| l.starts_with("void board_packet_start("))
        .unwrap()
        .split("emela_type_")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let release: usize = header
        .lines()
        .find(|l| {
            l.starts_with("void emela_embedded_release_value_")
                && l.contains(&format!("emela_type_{ty} *"))
        })
        .unwrap()
        .trim_start_matches("void emela_embedded_release_value_")
        .split('(')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    (ty, release)
}

#[test]
fn source_tasks_own_packet_arguments_and_successes_through_scope_deferred_cancel_and_null_results()
{
    let output = output();
    let (ty, release) = packet_ids(&output);
    compile_and_run(
        output,
        &format!(
            r#"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
void test_cancel(EmelaFiber f);
int test_heap_empty(void);
static EmelaTask pending;
static emela_type_{ty} board_value;
static int produced,released,locked,isr;
uint32_t emela_board_event_lock(void) {{ int old=locked;locked=1;return old; }}
void emela_board_event_unlock(uint32_t p) {{ assert(locked);locked=(int)p; }}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {{ fprintf(stderr,"%d:%.*s\n",k,n,m);abort(); }}
void board_packet_start(EmelaTask t,emela_type_{ty} value) {{
  assert(!locked&&!isr);pending=t;board_value=value;emela_embedded_retain_value_{release}(&board_value);
}}
void board_text_start(EmelaTask t,EmelaString value) {{ (void)t;(void)value;abort(); }}
static void produce(void*p,emela_type_{ty}*value) {{ assert(!locked&&!isr);produced++;*value=*(emela_type_{ty}*)p; }}
static void other(void*p,EmelaString*out) {{ (void)p;(void)out;abort(); }}
static void release(void*p) {{ assert(!locked&&!isr);released++;EmelaString text=((emela_type_{ty}*)p)->payload.v0.f0.f0;assert(text.length==2&&!memcmp(text.data,"42",2));emela_embedded_release_value_{release}(p);memset(p,0,sizeof(emela_type_{ty})); }}
static void drain(void) {{ while(emela_scheduler_run(1)) {{}} }}
int main(void) {{
  for(int i=0;i<1000;i++) {{
    for(int route=0;route<3;route++) {{
      emela_type_{ty} input=seed(42),result={{0}};produced=released=0;
      int cancel=i%5==1,discard=i%3==0;
      EmelaFiber f;
      if(route==0) f=exchange_start(input,discard?NULL:&result);
      else if(route==1) f=scoped_start(input,discard?NULL:&result);
      else f=deferred_start(input,discard?NULL:&result);
      emela_embedded_release_value_{release}(&input);drain();
      assert(!emela_fiber_done(f)&&board_value.tag==0);
      isr=1;assert(!emela_task_complete_int(pending,42));
      assert(!emela_task_complete_value_{string}(pending,other,NULL,release));
      assert(emela_task_complete_value_{ty}(pending,produce,&board_value,release));
      assert(!emela_task_complete_value_{ty}(pending,produce,&board_value,release));
      assert(!produced&&!released);isr=0;
      if(cancel) {{ test_cancel(f); }}
      drain();assert(emela_fiber_done(f));emela_fiber_release(f);
      assert(produced==!cancel&&released==1);
      if(!discard&&!cancel) {{
        EmelaString text=result.payload.v0.f0.f0;
        assert(result.tag==0&&text.owner&&text.length==2&&!memcmp(text.data,"42",2));
        assert(result.payload.v0.f0.f1.tag==0&&result.payload.v0.f0.f1.payload.v0.f0.owner);
        emela_embedded_release_value_{release}(&result);
      }}
      assert(test_heap_empty());
    }}
  }}
}}
"#,
            string = 4
        ),
    );
}

#[test]
fn string_successes_snapshot_nul_bytes_before_the_context_is_released() {
    let output = output();
    let (ty, _) = packet_ids(&output);
    compile_and_run(
        output,
        &format!(
            r#"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
void test_cancel(EmelaFiber f);int test_heap_empty(void);
static EmelaTask pending;static char board_bytes[3];static int produced,released;
uint32_t emela_board_event_lock(void) {{ return 0; }}
void emela_board_event_unlock(uint32_t p) {{ (void)p; }}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {{ fprintf(stderr,"%d:%.*s\n",k,n,m);abort(); }}
void board_packet_start(EmelaTask t,emela_type_{ty} v) {{ (void)t;(void)v;abort(); }}
void board_text_start(EmelaTask t,EmelaString value) {{ assert(value.length==3);pending=t;memcpy(board_bytes,value.data,3); }}
static void produce(void*p,EmelaString*out) {{ produced++;*out=(EmelaString){{.data=p,.length=3}}; }}
static void release(void*p) {{ released++;memset(p,'x',3); }}
static void drain(void) {{ while(emela_scheduler_run(1)) {{}} }}
int main(void) {{ for(int i=0;i<1000;i++) {{
  char original[]={{'a','\0','b'}};EmelaString result={{0}};produced=released=0;
  int cancel=i%5==1,discard=i%3==0;
  EmelaFiber f=echoed_start((EmelaString){{.data=original,.length=3}},discard?NULL:&result);
  memset(original,'z',3);drain();assert(!memcmp(board_bytes,"a\0b",3));
  assert(emela_task_complete_value_{string}(pending,produce,board_bytes,release));
  if(cancel) {{ test_cancel(f); }}
      drain();assert(emela_fiber_done(f));emela_fiber_release(f);
  assert(produced==!cancel&&released==1);
  if(!discard&&!cancel) {{ assert(result.owner&&result.length==3&&!memcmp(result.data,"a\0b",3));emela_embedded_release(result.owner); }}
  assert(test_heap_empty());
}} }}
"#,
            string = 4
        ),
    );
}
