//! Preserve fixed-old emit.rs inline-layout and switch observations in independent IR.
use super::*;

fn exported(name: &str, ret: TypeId) -> Function {
    let mut function = Function::new(name);
    function.exported = true;
    function.export_symbol = Some(name.into());
    function.ret = ret;
    function
}
fn return_body(function: &mut Function, stmts: Vec<StmtId>, result: Operand) {
    function.body = Some(function.seqs.alloc(Seq {
        stmts,
        term: Term::Return(Some(result)),
    }));
}
fn maybe(program: &mut Program) -> (EnumId, TypeId) {
    let parameter = program.core.types.intern(Type::Param(0));
    let id = program.core.enums.alloc(EnumDef {
        name: "Maybe".into(),
        params: 1,
        variants: vec![
            VariantDef {
                name: "Some".into(),
                fields: VariantFields::Positional(1),
                tys: vec![parameter],
            },
            VariantDef {
                name: "None".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
        error: false,
        external: false,
    });
    (id, program.core.types.enum_type(id, vec![TypeId::INT64]))
}
fn color(program: &mut Program) -> (EnumId, TypeId) {
    let id = program.core.enums.alloc(EnumDef {
        name: "Color".into(),
        params: 0,
        variants: ["Red", "Blue"]
            .into_iter()
            .map(|name| VariantDef {
                name: name.into(),
                fields: VariantFields::Unit,
                tys: vec![],
            })
            .collect(),
        error: false,
        external: false,
    });
    (id, program.core.types.enum_type(id, vec![]))
}

#[test]
fn legacy_inline_layouts_preserve_generic_fields_tags_and_unit_positions_at_the_c_boundary() {
    let mut program = program(vec![]);
    let (maybe_id, maybe_type) = maybe(&mut program);
    let (color_id, color_type) = color(&mut program);
    let parameter = program.core.types.intern(Type::Param(0));
    let boxed_id = program.core.enums.alloc(EnumDef {
        name: "Boxed".into(),
        params: 1,
        variants: vec![VariantDef {
            name: "Box".into(),
            fields: VariantFields::Positional(1),
            tys: vec![parameter],
        }],
        error: false,
        external: false,
    });
    let boxed_type = program.core.types.enum_type(boxed_id, vec![TypeId::INT64]);
    let named_id = program.core.enums.alloc(EnumDef {
        name: "Named".into(),
        params: 0,
        variants: vec![VariantDef {
            name: "Pair".into(),
            fields: VariantFields::Named(vec!["left".into(), "right".into()]),
            tys: vec![TypeId::INT, TypeId::INT64],
        }],
        error: false,
        external: false,
    });
    let named_type = program.core.types.enum_type(named_id, vec![]);
    let tuple_type =
        program
            .core
            .types
            .intern(Type::Tuple(vec![TypeId::INT64, TypeId::UNIT, TypeId::BOOL]));
    for (name, variant, args) in [
        ("some_value", 0, vec![Const::Int64(42).into()]),
        ("none_value", 1, vec![]),
    ] {
        let mut function = exported(name, maybe_type);
        let result = aggregate_variable(&mut function, maybe_type);
        let make = binding(
            &mut function,
            Some(result),
            Value::Make {
                ctor: CtorRef {
                    enum_id: maybe_id,
                    variant,
                },
                args,
            },
        );
        return_body(&mut function, vec![make], result.into());
        program.functions.alloc(function);
    }
    let mut field = exported("generic_field", TypeId::INT64);
    let input = aggregate_variable(&mut field, boxed_type);
    field.params.push(input);
    let result = variable(&mut field, TypeId::INT64);
    let read = binding(
        &mut field,
        Some(result),
        Value::Field {
            base: input,
            ctor: CtorRef {
                enum_id: boxed_id,
                variant: 0,
            },
            index: 0,
        },
    );
    return_body(&mut field, vec![read], result.into());
    program.functions.alloc(field);

    let mut boxed = exported("boxed_value", boxed_type);
    let result = aggregate_variable(&mut boxed, boxed_type);
    let make = binding(
        &mut boxed,
        Some(result),
        Value::Make {
            ctor: CtorRef {
                enum_id: boxed_id,
                variant: 0,
            },
            args: vec![Const::Int64(73).into()],
        },
    );
    return_body(&mut boxed, vec![make], result.into());
    program.functions.alloc(boxed);

    let mut named = exported("named_pair", named_type);
    let result = aggregate_variable(&mut named, named_type);
    let make = binding(
        &mut named,
        Some(result),
        Value::Make {
            ctor: CtorRef {
                enum_id: named_id,
                variant: 0,
            },
            args: vec![Const::Int(7).into(), Const::Int64(42).into()],
        },
    );
    return_body(&mut named, vec![make], result.into());
    program.functions.alloc(named);

    let mut tag = exported("payload_tag", TypeId::INT);
    let input = aggregate_variable(&mut tag, maybe_type);
    tag.params.push(input);
    let result = variable(&mut tag, TypeId::INT);
    let read = binding(&mut tag, Some(result), Value::Tag(input));
    return_body(&mut tag, vec![read], result.into());
    program.functions.alloc(tag);

    let mut tag = exported("nullary_tag", TypeId::INT);
    let input = tag.vars.alloc(VarData {
        hint: "blue".into(),
        ty: color_type,
        repr: Repr::I32,
    });
    let result = variable(&mut tag, TypeId::INT);
    let make = binding(
        &mut tag,
        Some(input),
        Value::Make {
            ctor: CtorRef {
                enum_id: color_id,
                variant: 1,
            },
            args: vec![],
        },
    );
    let read = binding(&mut tag, Some(result), Value::Tag(input));
    return_body(&mut tag, vec![make, read], result.into());
    program.functions.alloc(tag);

    let mut tuple = exported("tuple_value", tuple_type);
    let result = aggregate_variable(&mut tuple, tuple_type);
    let make = binding(
        &mut tuple,
        Some(result),
        Value::Tuple(vec![
            Const::Int64(91).into(),
            Const::Unit.into(),
            Const::Bool(true).into(),
        ]),
    );
    return_body(&mut tuple, vec![make], result.into());
    program.functions.alloc(tuple);
    let mut project = exported("tuple_field", TypeId::BOOL);
    let input = aggregate_variable(&mut project, tuple_type);
    project.params.push(input);
    let ignored_unit = binding(
        &mut project,
        None,
        Value::TupleGet {
            base: input,
            index: 1,
        },
    );
    let result = variable(&mut project, TypeId::BOOL);
    let read = binding(
        &mut project,
        Some(result),
        Value::TupleGet {
            base: input,
            index: 2,
        },
    );
    return_body(&mut project, vec![ignored_unit, read], result.into());
    program.functions.alloc(project);

    let output = crate::backend::compile_program(program, &Options::default()).unwrap();
    let maybe_c = crate::semantics::c::type_name(maybe_type);
    let boxed_c = crate::semantics::c::type_name(boxed_type);
    let named_c = crate::semantics::c::type_name(named_type);
    let tuple_c = crate::semantics::c::type_name(tuple_type);
    execute(
        &output,
        &format!(
            r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)k;(void)m;(void)n;abort();}}
int main(void){{for(int i=0;i<1000;i++){{
  {maybe_c} some=some_value();assert(some.tag==0 && some.payload.v0.f0==INT64_C(42));
  {maybe_c} none=none_value();assert(none.tag==1);
  {boxed_c} wrapped=boxed_value();
  assert(wrapped.tag==0 && wrapped.payload.v0.f0==INT64_C(73) && generic_field(wrapped)==INT64_C(73));
  {named_c} named=named_pair();assert(named.payload.v0.f0==7 && named.payload.v0.f1==INT64_C(42));
  assert(payload_tag(some)==0 && payload_tag(none)==1 && nullary_tag()==1);
  {tuple_c} tuple=tuple_value();assert(tuple.f0==INT64_C(91) && tuple.f2==1 && tuple_field(tuple)==1);
}}return 0;}}
"#
        ),
        0,
    );
}

fn literal_switch(
    program: &mut Program,
    name: &str,
    ty: TypeId,
    cases: Vec<(Const, i32)>,
    fallback: i32,
) {
    let mut function = exported(name, TypeId::INT);
    let input = variable(&mut function, ty);
    function.params.push(input);
    let result = variable(&mut function, TypeId::INT);
    let cases = cases
        .into_iter()
        .map(|(value, answer)| Case {
            key: CaseKey::Lit(value),
            body: function.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Yield(Some(Const::Int(answer).into())),
            }),
        })
        .collect();
    let default = function.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(Some(Const::Int(fallback).into())),
    });
    let switch = function.stmts.alloc(Stmt::Switch {
        dst: Some(result),
        on: SwitchOn::Lit(input),
        cases,
        default: Some(default),
    });
    return_body(&mut function, vec![switch], result.into());
    program.functions.alloc(function);
}
fn enum_switch(program: &mut Program, name: &str, ty: TypeId, repr: Repr, answers: [i32; 2]) {
    let mut function = exported(name, TypeId::INT);
    let input = function.vars.alloc(VarData {
        hint: "input".into(),
        ty,
        repr,
    });
    function.params.push(input);
    let result = variable(&mut function, TypeId::INT);
    let cases = answers
        .into_iter()
        .enumerate()
        .map(|(variant, answer)| Case {
            key: CaseKey::Variant(variant),
            body: function.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Yield(Some(Const::Int(answer).into())),
            }),
        })
        .collect();
    let switch = function.stmts.alloc(Stmt::Switch {
        dst: Some(result),
        on: SwitchOn::Tag(input),
        cases,
        default: None,
    });
    return_body(&mut function, vec![switch], result.into());
    program.functions.alloc(function);
}

#[test]
fn legacy_literal_and_enum_switches_cover_numeric_extremes_and_reject_invalid_tags() {
    let mut program = program(vec![]);
    literal_switch(
        &mut program,
        "choose_int",
        TypeId::INT,
        vec![(Const::Int(i32::MIN), 11), (Const::Int(0), 22)],
        33,
    );
    literal_switch(
        &mut program,
        "choose_int64",
        TypeId::INT64,
        vec![(Const::Int64(i64::MIN), 44), (Const::Int64(i64::MAX), 55)],
        66,
    );
    let (_, maybe_type) = maybe(&mut program);
    let (_, color_type) = color(&mut program);
    enum_switch(&mut program, "classify", maybe_type, Repr::Ref, [71, 82]);
    enum_switch(&mut program, "color_code", color_type, Repr::I32, [3, 5]);
    let output = crate::backend::compile_program(program, &Options::default()).unwrap();
    let maybe_c = crate::semantics::c::type_name(maybe_type);
    execute(
        &output,
        &format!(
            r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int expected_invalid;
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char*message,uint16_t length){{
  if(expected_invalid && kind==EMELA_DEFECT_NO_MATCH && length==8 && !memcmp(message,"no match",8))exit(0);
  exit(1);
}}
int main(void){{for(int i=0;i<1000;i++){{
  assert(choose_int(INT32_MIN)==11 && choose_int(0)==22 && choose_int(1)==33);
  assert(choose_int64(INT64_MIN)==44 && choose_int64(INT64_MAX)==55 && choose_int64(0)==66);
  {maybe_c} some={{.tag=0,.payload.v0.f0=INT64_C(7)}};{maybe_c} none={{.tag=1}};
  assert(classify(some)==71 && classify(none)==82 && color_code(0)==3 && color_code(1)==5);
}}
expected_invalid=1;{maybe_c} invalid={{.tag=7}};(void)classify(invalid);return 2;}}
"#
        ),
        0,
    );
}
