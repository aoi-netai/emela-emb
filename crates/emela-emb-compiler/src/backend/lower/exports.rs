//! External C exports call a private foreign declaration through an ordinary wrapper.
use super::*;

pub(super) fn normalize(program: &mut Program) {
    let external_exports: Vec<_> = program
        .functions
        .iter()
        .filter_map(|(id, function)| {
            (function.exported && function.external.is_some()).then_some(id)
        })
        .collect();
    for id in external_exports {
        let original = program.functions[id].clone();
        let mut target = original.clone();
        target.exported = false;
        target.export_symbol = None;
        target.name = format!("{}$external_{}", original.name, u32::from(id.into_raw()));
        let target = program.functions.alloc(target);
        let mut wrapper = Function::new(&original.name);
        wrapper.exported = true;
        wrapper.export_symbol = original.export_symbol;
        wrapper.ret = original.ret;
        wrapper.fails = original.fails;
        wrapper.suspends = original.suspends;
        let mut args = Vec::new();
        for param in original.params {
            let param = wrapper.vars.alloc(original.vars[param].clone());
            wrapper.params.push(param);
            args.push(param.into());
        }
        let result = match program.core.ty(original.ret) {
            Type::Unit | Type::Never => None,
            ty => Some(wrapper.vars.alloc(VarData {
                hint: "result".into(),
                ty: original.ret,
                repr: match ty {
                    Type::Int | Type::Bool => Repr::I32,
                    Type::Int64 => Repr::I64,
                    Type::Float => Repr::F64,
                    _ => Repr::Ref,
                },
            })),
        };
        let call = wrapper.stmts.alloc(Stmt::Let {
            dst: result,
            value: Value::Call {
                callee: Callee::Direct(target),
                args,
                suspends: original.suspends,
                on_fail: original.fails.then_some(OnFail::Propagate),
            },
            on_defect: None,
            on_suspend: None,
        });
        wrapper.body = Some(wrapper.seqs.alloc(Seq {
            stmts: vec![call],
            term: Term::Return(result.map(Into::into)),
        }));
        program.functions[id] = wrapper;
    }
}
