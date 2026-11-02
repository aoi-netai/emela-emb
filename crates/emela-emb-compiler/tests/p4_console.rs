#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{ExternalAdapter, ExternalResult, Options, Source, compile_sources};

#[test]
fn standard_console_handlers_preserve_callback_effects_owned_lines_and_c_boundaries() {
    let source = r#"
import Std.Console.{Console, SystemConsole, TestConsole}
import Std.State.{State}
@export(c, "console_callbacks")
pub fn run(input: String) -> Int {
  (_, lines) = State.run([], fn() {
    with TestConsole {
      (use Console).print_line(input)
      List.map([20, 22], fn(n) {
        (use Console).print_line(Int.to_string(n))
        n
      })
      (use Console).print_error_line("discard")
    }
  })
  assert lines == [input, "20", "22"]
  with SystemConsole {
    List.map(lines, fn(line) { (use Console).print_line(line) })
    (use Console).print_error_line("error")
  }
  42
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(65536)
            .with_external_adapter(
                "emela_console_print_line",
                ExternalAdapter::new("board_console_print_line", ExternalResult::Owned),
            )
            .with_external_adapter(
                "emela_console_print_error_line",
                ExternalAdapter::new("board_console_print_error_line", ExternalResult::Owned),
            ),
    )
    .unwrap_or_else(|errors| panic!("Console source rejected: {errors:#?}"));
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int lines,errors;
void board_console_print_line(EmelaString value){
  const char*expected[]={"a\0b","20","22"};
  const int sizes[]={3,2,2};
  assert(lines<3&&value.length==sizes[lines]&&!memcmp(value.data,expected[lines],value.length));lines++;
}
void board_console_print_error_line(EmelaString value){assert(value.length==5&&!memcmp(value.data,"error",5));errors++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){lines=errors=0;assert(console_callbacks((EmelaString){"a\0b",3,NULL})==42);assert(lines==3&&errors==1&&test_heap_empty());}return 0;}
"#,
    );
}

#[test]
fn reserved_standard_c_symbols_cannot_be_selected_by_user_declarations() {
    for symbol in [
        "emela_console_print_line",
        "emela_console_print_error_line",
        "emela_embedded_release",
    ] {
        let source = format!("@external(c,\"{symbol}\")\nfn console_log(value:String)->Unit");
        let errors = emela_emb_compiler::compile(&source).unwrap_err();
        assert!(
            errors.iter().any(
                |error| error.kind == emela_emb_compiler::DiagnosticKind::Export
                    && error.message.contains("non-reserved")
            ),
            "{errors:#?}"
        );
    }
}
