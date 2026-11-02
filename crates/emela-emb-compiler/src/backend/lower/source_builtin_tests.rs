//! Source operations must be normalized before the independent backend boundary.
use super::*;

const SOURCE_ONLY: [Builtin; 8] = [
    Builtin::AssertFail,
    Builtin::Dbg,
    Builtin::DebugPrint,
    Builtin::Inspect,
    Builtin::Panic,
    Builtin::ShowNested,
    Builtin::ShowString,
    Builtin::Todo,
];

#[test]
fn rejects_all_eight_residual_source_builtins_before_emission() {
    for op in SOURCE_ONLY {
        let mut f = simple();
        f.ret = TypeId::UNIT;
        let args = match op {
            Builtin::Todo => vec![],
            Builtin::AssertFail => vec![
                Const::String("assert".into()).into(),
                Const::String("main.emel:1:1".into()).into(),
            ],
            Builtin::Dbg | Builtin::Inspect | Builtin::ShowNested => vec![Const::Int(42).into()],
            _ => vec![Const::String("debug".into()).into()],
        };
        let statement = binding(&mut f, None, Value::Builtin { op, args });
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![statement],
            term: Term::Return(None),
        };
        let errors =
            crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == crate::ErrorKind::Unsupported),
            "{op:?}: {errors:?}"
        );
    }
}

#[test]
fn source_debug_display_assert_and_defect_operations_leave_no_residual_builtins() {
    use crate::frontend;
    let source = r#"
type Item(value:String) derive Show
@export(c,"display")
pub fn display(value:String)->String {
 assert value!="bad"
 item=dbg(Item(value))
 inspect((item,value))
}
@export(c,"stop")
pub fn stop(value:String)->Int {panic(value)}
@export(c,"unfinished")
pub fn unfinished()->Int {todo()}
"#;
    let mut modules =
        frontend::modules::collect(&[crate::Source::new("main.emel", source)], "main.emel")
            .unwrap();
    frontend::consts::expand(&mut modules).unwrap();
    let checked = frontend::check::check(modules).unwrap();
    let mut checked = frontend::specialize::specialize(checked).unwrap();
    frontend::function_abi::project(&mut checked).unwrap();
    let p = frontend::lower::lower(&checked);
    let mut show = false;
    let mut debug = false;
    let mut defect = false;
    for (_, f) in p.functions.iter() {
        for (_, statement) in f.stmts.iter() {
            if let Stmt::Let { value, .. } = statement {
                match value {
                    Value::Builtin { op, .. } => {
                        assert!(!SOURCE_ONLY.contains(op), "residual {op:?}")
                    }
                    Value::Runtime {
                        op: RuntimeOp::ShowString,
                        ..
                    } => show = true,
                    Value::Runtime {
                        op: RuntimeOp::DebugPrint,
                        ..
                    } => debug = true,
                    _ => {}
                }
            }
        }
        defect |= f
            .seqs
            .iter()
            .any(|(_, seq)| matches!(seq.term, Term::Defect(_)));
    }
    assert!(show && debug && defect);
    crate::backend::compile_program(p, &Options::default()).unwrap();
}
