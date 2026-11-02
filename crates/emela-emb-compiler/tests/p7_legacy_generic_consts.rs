//! Fixed old const inputs retain their bodies; only public C exports are added.
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

const FIXTURES: &[(&str, &str)] = &[
    (
        "090d8d",
        include_str!("compat/p7/generic-consts/090d8d1bc5a5b8ddd694.emel"),
    ),
    (
        "0e55f9",
        include_str!("compat/p7/generic-consts/0e55f902c5bac78b6187.emel"),
    ),
    (
        "1e18a1",
        include_str!("compat/p7/generic-consts/1e18a161ce360199a21e.emel"),
    ),
    (
        "4ec3db",
        include_str!("compat/p7/generic-consts/4ec3db5b593e7e155054.emel"),
    ),
    (
        "6509a2",
        include_str!("compat/p7/generic-consts/6509a26b86dc042348d0.emel"),
    ),
    (
        "82d224",
        include_str!("compat/p7/generic-consts/82d224825b4fecc59779.emel"),
    ),
    (
        "9d483c",
        include_str!("compat/p7/generic-consts/9d483cae98bfe8662280.emel"),
    ),
    (
        "cd203f",
        include_str!("compat/p7/generic-consts/cd203f34d6ed62e80e19.emel"),
    ),
];

#[test]
fn fixed_legacy_generic_const_inputs_execute_through_the_independent_compiler() {
    for &(id, original) in FIXTURES {
        let mut source = String::new();
        for line in original.lines() {
            if let Some(rest) = line.strip_prefix("pub fn ") {
                let name = rest.split('(').next().unwrap();
                source.push_str(&format!("@export(c,\"{name}\")\n"));
            }
            source.push_str(line);
            source.push('\n');
        }
        let mut output = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default().with_heap_bytes(32768),
        )
        .unwrap_or_else(|errors| panic!("legacy {id}: {errors:#?}"));
        let heap_check = if output.c_source.contains("emela_heap_storage") {
            "int test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n"
        } else {
            "int test_heap_empty(void){return 1;}\n"
        };
        output
            .files
            .iter_mut()
            .find(|file| file.path == "emela_embedded.c")
            .unwrap()
            .contents
            .push_str(heap_check);
        let headers = "#include <stdlib.h>\n#include <setjmp.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n";
        let abort = "_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\n";
        let debug = "static int logs;void emela_board_debug(const char*s,uint16_t n){(void)s;(void)n;logs++;}\n";
        let (hooks, main) = match id {
            "090d8d" => (abort.to_owned(), "int main(void){for(int i=0;i<1000;i++)if(work()!=42||!test_heap_empty())return 1;return 0;}\n".to_owned()),
            "0e55f9" => (format!("{abort}{debug}"), "int main(void){for(int i=0;i<1000;i++)if(text()!=42||integer()!=42||logs!=2)return 1;return 0;}\n".to_owned()),
            "1e18a1" => (abort.to_owned(), "int main(void){for(int i=0;i<1000;i++){EmelaString s=text();if(integer()!=42||s.length!=2||memcmp(s.data,\"42\",2))return 1;}return 0;}\n".to_owned()),
            "4ec3db" | "6509a2" => {
                let recursive = id == "6509a2";
                let expected = if recursive {"const BROKEN refers to itself while being evaluated"} else {"while evaluating const BROKEN: broken"};
                let hooks = format!("{debug}\nstatic jmp_buf jump;static int calls,defects;int32_t trigger(void){{calls++;return 1;}}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;const char expected[]=\"{expected}\";if(n!=sizeof(expected)-1||memcmp(m,expected,n))abort();defects++;longjmp(jump,1);}}\n");
                let count = if recursive {2000} else {1};
                let main = format!("int main(void){{for(volatile int i=0;i<1000;i++){{if(setjmp(jump)==0){{integer();abort();}}if(setjmp(jump)==0){{text();abort();}}if(!test_heap_empty())return 1;}}return calls!={count}||logs!={count}||defects!=2000;}}\n");
                (hooks, main)
            },
            "82d224" => (format!("{abort}{debug}"), "int main(void){for(int i=0;i<1000;i++)if(integer()!=42||text()!=42||logs!=1)return 1;return 0;}\n".to_owned()),
            "9d483c" => (format!("{abort}{debug}"), "int main(void){for(int i=0;i<1000;i++)if(read()!=0||logs!=1||!test_heap_empty())return 1;return 0;}\n".to_owned()),
            "cd203f" => (format!("{abort}{debug}\nstatic int ticks,marks;int32_t tick(void){{return ++ticks;}}void mark(int32_t n){{if(n!=2)abort();marks++;}}\n"), "int main(void){for(int i=0;i<1000;i++)if(integer()!=42||text()!=42||ticks!=2||logs!=6||marks!=2*(i+1))return 1;return 0;}\n".to_owned()),
            _ => unreachable!(),
        };
        support::compile_and_run(&output, &format!("{headers}{hooks}{main}"));
    }
}
