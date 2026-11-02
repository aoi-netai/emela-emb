mod support;
use emela_emb_compiler::{Options, Output, Source, compile_sources};

fn output() -> Output {
    let source = r##"
@external(c,"stack_mark")
fn stack_mark()->()
fn pad(s:String,width:Int)->String {
  if String.length(s)>=width {s} else {pad("0#{s}",width)}
}
fn append_to(s:String,width:Int)->String {
  if String.length(s)>=width {s} else {append_to("#{s}0",width)}
}
fn double(s:String,count:Int)->String {
  if count==0 {s} else {double("#{s}#{s}",count-1)}
}
fn trim_loop(s:String,count:Int)->String {
  if count==0 {s} else {
    stack_mark()
    trim_loop(String.trim(" #{s} "),count-1)
  }
}
@export(c,"append_to_output")
pub fn append_to_output(a:String,b:String)->String {String.append(a,b)}
@export(c,"string_alias")
pub fn run()->Int {
  assert pad("1",6)=="000001"
  assert append_to("1",6)=="100000"
  assert double("1",3)=="11111111"
  assert trim_loop("1",100000)=="1"
  42
}
"##;
    compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_string_capacity(64),
    )
    .unwrap()
}

#[test]
fn tail_recursive_string_views_preserve_prepend_append_and_concat_without_heap_or_stack_growth() {
    let output = output();
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static uintptr_t first_stack;
void stack_mark(void){unsigned char marker=0;uintptr_t current=(uintptr_t)(void*)&marker;if(first_stack==0)first_stack=current;else if(first_stack!=current)abort();}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){char output[1024];EmelaString joined=append_to_output((EmelaString){.data="left",.length=4},(EmelaString){.data="right",.length=5},output,1024);if(joined.length!=9||memcmp(joined.data,"leftright",9))return 2;for(int i=0;i<10;i++){first_stack=0;if(string_alias()!=42||first_stack==0)return 1;}return 0;}
"#,
    );
}

#[test]
fn append_handles_both_inputs_overlapping_the_output_and_the_capacity_limits() {
    let mut output = output();
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    source.push_str(r#"
#include <string.h>
int test_append_aliases(void){
  char buffer[512];EmelaString value;
  memcpy(buffer,"abcd",4);value=emela_emb_string_append((EmelaString){.data="0",.length=1},(EmelaString){.data=buffer,.length=4},buffer,64,1);
  if(value.length!=5||memcmp(buffer,"0abcd",5))return 1;
  memcpy(buffer,"abcd",4);value=emela_emb_string_append((EmelaString){.data=buffer,.length=4},(EmelaString){.data="0",.length=1},buffer,64,0);
  if(value.length!=5||memcmp(buffer,"abcd0",5))return 2;
  memcpy(buffer,"abcd",4);value=emela_emb_string_append((EmelaString){.data=buffer,.length=4},(EmelaString){.data=buffer,.length=4},buffer,64,0);
  if(value.length!=8||memcmp(buffer,"abcdabcd",8))return 3;
  memcpy(buffer,"abcd",4);value=emela_emb_string_append((EmelaString){.data=buffer+2,.length=2},(EmelaString){.data=buffer,.length=3},buffer,64,1);
  if(value.length!=5||memcmp(buffer,"cdabc",5))return 4;
  memcpy(buffer,"abcd",4);value=emela_emb_string_append((EmelaString){.data=buffer+1,.length=3},(EmelaString){.data=buffer,.length=4},buffer,64,1);
  if(value.length!=7||memcmp(buffer,"bcdabcd",7))return 5;
  memcpy(buffer,"abcd",4);value=emela_emb_string_append((EmelaString){.data=buffer,.length=4},(EmelaString){.data=buffer+2,.length=2},buffer+1,64,1);
  if(value.length!=6||memcmp(buffer+1,"abcdcd",6))return 6;
  memset(buffer,'x',512);value=emela_emb_string_append((EmelaString){.data=buffer,.length=256},(EmelaString){.data=buffer+128,.length=256},buffer,512,0);
  if(value.length!=512){return 7;}
  for(unsigned i=0;i<512;i++){if(buffer[i]!='x'){return 8;}}
  buffer[0]='a';value=emela_emb_string_append((EmelaString){.data=buffer,.length=1},(EmelaString){.data=buffer,.length=0},buffer,1,0);
  if(value.length!=1||buffer[0]!='a'){return 9;}
  return 0;
}
void test_append_bad_capacity(uint16_t capacity){char buffer[512];(void)emela_emb_string_append((EmelaString){.data="a",.length=1},(EmelaString){0},buffer,capacity,1);}
"#);
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_append_aliases(void);
void stack_mark(void){}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){return test_append_aliases();}
"#,
    );
    for capacity in [0, 513] {
        support::compile_and_run(
            &output,
            &format!(
                r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
void test_append_bad_capacity(uint16_t capacity);
void stack_mark(void){{}}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)m;(void)n;exit(k==EMELA_DEFECT_STRING_CAPACITY?0:2);}}
int main(void){{test_append_bad_capacity({capacity});return 1;}}
"#
            ),
        );
        assert!(
            compile_sources(
                &[Source::new("main.emel", "fn main(){}")],
                "main.emel",
                &Options::default().with_string_capacity(capacity)
            )
            .is_err()
        );
    }
}
