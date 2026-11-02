#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn suspended_runtime_error_displays_preserve_show_and_concrete_generic_payloads() {
    let source = r#"
error Written(value:String)
impl Written:Show {fn show(self){"written<#{self.value}>"}}
error Derived(message:String) derive Show
error Plain(secret:String)
error Payload[A](value:A) derive Show
@export(c,"produce")
pub fn produce(which:Int)->Int fails Written|Derived|Plain|Payload[Int]|Payload[String] {
  if which==0 {fail Written(value:String.append("x","!"))}
  else if which==1 {fail Derived(message:String.append("x","!"))}
  else if which==2 {fail Plain(secret:String.append("hidden","!"))}
  else if which==3 {fail Payload(value:42)}
  else {fail Payload(value:String.append("x","!"))}
}
effect Wait {suspend fn get()->Int}
handler Immediate:Wait {suspend fn get(resume){resume(42)}}
@export(c,"resume_enabled")
pub fn run()->Int {with Immediate {(use Wait).get()}}
"#;
    let mut output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(16384),
    )
    .unwrap();
    // Exercise the diagnostic helper emitted for resumable operations without
    // triggering a terminal defect, so every formatted payload can be released.
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(
            "\nuint16_t test_display(int32_t which,char*buffer,uint16_t capacity){EmelaError error={0};(void)produce_impl(which,&error);EmelaString text=emela_error_to_string(error,buffer,capacity);emela_error_clear(&error);return text.length;}\n",
        );
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
uint16_t test_display(int32_t,char*,uint16_t);
int test_heap_empty(void);
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){
 const char*expected[]={"error: written<x!>","error: Derived(message: \"x!\")","error: Plain","error: Payload(value: 42)","error: Payload(value: \"x!\")"};
 for(int i=0;i<1000;i++){
  assert(resume_enabled()==42);
  for(int which=0;which<5;which++){
   char buffer[256];uint16_t length=test_display(which,buffer,sizeof(buffer));
   assert(length==strlen(expected[which])&&!memcmp(buffer,expected[which],length));
   assert(test_heap_empty());
  }
 }
 return 0;
}
"#,
    );
}
