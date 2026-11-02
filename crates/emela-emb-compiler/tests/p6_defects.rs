mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn attempts_stop_at_defects_without_running_unreachable_handler_release() {
    let original = include_str!("compat/p6/attempt-defect.emel");
    for dynamic in [false, true] {
        let original = if dynamic {
            original.replace(
                "panic(\"test defect\")",
                "panic(String.append(\"test \",\"defect\"))",
            )
        } else {
            original.to_owned()
        };
        let source = original.replace(
            "pub fn stopped()",
            "@export(c,\"stopped\")\npub fn stopped()",
        );
        let output = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(32768)
                .with_frame_bytes(1024),
        )
        .unwrap();
        support::compile_and_run(
            &output,
            r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
void mark(int32_t n){(void)n;abort();}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){if(k!=EMELA_DEFECT_PANIC||n!=11||memcmp(m,"test defect",11))abort();exit(0);}
int main(void){(void)stopped();abort();}
"#,
        );
    }
}

#[test]
fn typed_failure_before_a_defect_still_runs_the_protected_release() {
    let original = include_str!("compat/p6/attempt-defect.emel");
    let source = format!(
        "{}\nerror Oops\nfn fails_now()->Int fails Oops{{fail Oops}}\n",
        original
            .replace(
                "pub fn stopped()",
                "@export(c,\"stopped\")\npub fn stopped()"
            )
            .replace(
                "panic(\"test defect\")",
                "fails_now()\n panic(\"test defect\")"
            )
    );
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(32768)
            .with_frame_bytes(1024),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int releases;
void mark(int32_t n){if(n!=99)abort();releases++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++)if(stopped()!=2||releases!=i+1)return 1;return 0;}
"#,
    );
}
