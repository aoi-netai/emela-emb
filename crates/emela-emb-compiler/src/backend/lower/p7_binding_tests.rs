//! Fixed-old tests/binding_metadata/mod.rs:4, using only independent IR metadata.
use super::*;

#[test]
fn global_named_file_and_bundled_host_metadata_preserve_the_external_c_symbol() {
    let mut program = program(vec![]);
    let hosts = [
        HostRef::Global("read_number".into()),
        HostRef::Import {
            module: HostModule::Named("package".into()),
            name: "read_number".into(),
        },
        HostRef::Import {
            module: HostModule::File("binding.c".into()),
            name: "read_number".into(),
        },
        HostRef::Import {
            module: HostModule::Bundled("embedded".into()),
            name: "read_number".into(),
        },
    ];
    for (index, host) in hosts.into_iter().enumerate() {
        let mut external = Function::new(&format!("binding_{index}"));
        external.ret = TypeId::INT;
        external.external = Some(External {
            bindings: vec![ExternalBinding {
                target: "c".into(),
                host,
            }],
        });
        let target = program.functions.alloc(external);
        let mut entry = simple();
        entry.name = format!("read_{index}");
        entry.export_symbol = Some(entry.name.clone());
        let result = variable(&mut entry, TypeId::INT);
        let call = binding(
            &mut entry,
            Some(result),
            Value::Call {
                callee: Callee::Direct(target),
                args: vec![],
                suspends: false,
                on_fail: None,
            },
        );
        entry.seqs[entry.body.unwrap()] = Seq {
            stmts: vec![call],
            term: Term::Return(Some(result.into())),
        };
        program.functions.alloc(entry);
    }
    let output = crate::backend::compile_program(program, &Options::default()).unwrap();
    execute(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int calls;
int32_t read_number(void) { calls++;return 42; }
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){
  assert(read_0()==42 && read_1()==42 && read_2()==42 && read_3()==42);
  assert(calls==4*(i+1));
}return 0;}
"#,
        0,
    );
}
