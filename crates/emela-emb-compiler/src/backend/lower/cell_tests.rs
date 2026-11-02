//! Independent IR equivalents of the fixed legacy tests/cell/mod.rs boundaries.
use super::*;

fn cell_variable(function: &mut Function, ty: TypeId) -> VarId {
    function.vars.alloc(VarData {
        hint: String::new(),
        ty,
        repr: Repr::Ref,
    })
}
fn cell_binding(
    function: &mut Function,
    dst: Option<VarId>,
    op: Builtin,
    args: Vec<Operand>,
) -> StmtId {
    binding(function, dst, Value::Builtin { op, args })
}
fn execute_cells(program: Program, body: &str) {
    let mut output =
        crate::backend::compile_program(program, &Options::default().with_heap_bytes(1024))
            .unwrap();
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
    execute(
        &output,
        &format!(
            r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{fprintf(stderr,"%d: %.*s\n",k,n,m);abort();}}
int main(void){{for(unsigned i=0;i<1000;i++){{{body}}}return 0;}}
"#
        ),
        0,
    );
}

#[test]
fn unit_cells_have_shared_identity_and_release_after_the_last_external_root() {
    let mut p = program(vec![]);
    let ty = p.core.types.intern(Type::Cell(TypeId::UNIT));
    let mut f = simple();
    f.ret = ty;
    let cell = cell_variable(&mut f, ty);
    let make = cell_binding(
        &mut f,
        Some(cell),
        Builtin::CellNew,
        vec![Const::Unit.into()],
    );
    let read = cell_binding(&mut f, None, Builtin::CellGet, vec![cell.into()]);
    let write = cell_binding(
        &mut f,
        None,
        Builtin::CellSet,
        vec![cell.into(), Const::Unit.into()],
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![make, read, write],
        term: Term::Return(Some(cell.into())),
    };
    p.functions.alloc(f);
    execute_cells(
        p,
        "void*value=answer();assert(value&&!test_heap_empty());emela_embedded_retain(value);emela_embedded_release(value);assert(!test_heap_empty());emela_embedded_release(value);assert(test_heap_empty());",
    );
}

#[test]
fn discarded_cell_construction_and_reads_release_owned_contents() {
    let mut p = program(vec![]);
    let ty = p.core.types.intern(Type::Cell(TypeId::INT));
    p.core.types.intern(Type::Cell(TypeId::STRING));
    let mut f = simple();
    f.ret = TypeId::UNIT;
    let text = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(text);
    let cell = cell_variable(&mut f, ty);
    let discarded = cell_binding(&mut f, None, Builtin::CellNew, vec![text.into()]);
    let make = cell_binding(
        &mut f,
        Some(cell),
        Builtin::CellNew,
        vec![Const::Int(42).into()],
    );
    let read = cell_binding(&mut f, None, Builtin::CellGet, vec![cell.into()]);
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![discarded, make, read],
        term: Term::Return(None),
    };
    p.functions.alloc(f);
    execute_cells(
        p,
        "answer((EmelaString){\"hello\",5,NULL});assert(test_heap_empty());",
    );
}

#[test]
fn cell_references_in_error_payloads_outlive_the_failing_frame() {
    let mut p = program(vec![]);
    let cell_ty = p.core.types.intern(Type::Cell(TypeId::INT));
    let error_id = p.core.enums.alloc(EnumDef {
        name: "Stored".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "Stored".into(),
            fields: VariantFields::Positional(1),
            tys: vec![cell_ty],
        }],
        error: true,
        external: false,
    });
    let error_ty = p.core.types.intern(Type::Enum(error_id, vec![]));
    let ctor = CtorRef {
        enum_id: error_id,
        variant: 0,
    };
    let mut raise = Function::new("raise_cell");
    raise.fails = true;
    raise.ret = TypeId::INT;
    let cell = cell_variable(&mut raise, cell_ty);
    let error = aggregate_variable(&mut raise, error_ty);
    let make = cell_binding(
        &mut raise,
        Some(cell),
        Builtin::CellNew,
        vec![Const::Int(42).into()],
    );
    let wrap = binding(
        &mut raise,
        Some(error),
        Value::Make {
            ctor,
            args: vec![cell.into()],
        },
    );
    raise.body = Some(raise.seqs.alloc(Seq {
        stmts: vec![make, wrap],
        term: Term::Fail(error.into()),
    }));
    let target = p.functions.alloc(raise);
    let mut f = simple();
    let caught = aggregate_variable(&mut f, TypeId::ANY);
    let concrete = aggregate_variable(&mut f, error_ty);
    let saved = cell_variable(&mut f, cell_ty);
    let value = variable(&mut f, TypeId::INT);
    let result = variable(&mut f, TypeId::INT);
    let unpack = binding(
        &mut f,
        Some(concrete),
        Value::AsError {
            error: caught,
            enum_id: error_id,
            error_type: error_ty,
        },
    );
    let field = binding(
        &mut f,
        Some(saved),
        Value::Field {
            base: concrete,
            ctor,
            index: 0,
        },
    );
    let read = cell_binding(&mut f, Some(value), Builtin::CellGet, vec![saved.into()]);
    let recover = f.seqs.alloc(Seq {
        stmts: vec![unpack, field, read],
        term: Term::Yield(Some(value.into())),
    });
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Direct(target),
            args: vec![],
            suspends: false,
            on_fail: Some(OnFail::Handle {
                error: caught,
                body: recover,
            }),
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    execute_cells(p, "assert(answer()==42&&test_heap_empty());");
}
