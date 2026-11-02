use super::*;

#[test]
fn retirement_keeps_active_checks_through_all_aliases_in_native_and_suspend_callers() {
    for suspends in [false, true] {
        for kind in ["direct", "evidence", "coerce", "tuple", "argument"] {
            let mut p = program(vec![]);
            let (effect, ht, ft) = read_effect(&mut p);
            let callback = callback(&mut p);
            let mut identity = Function::new("identity_handler");
            identity.ret = ht;
            let input = reference(&mut identity, ht);
            identity.params.push(input);
            identity.body = Some(identity.seqs.alloc(Seq {
                stmts: vec![],
                term: Term::Return(Some(input.into())),
            }));
            let identity = p.functions.alloc(identity);
            let mut release = Function::new("release");
            let input = reference(&mut release, ht);
            release.params.push(input);
            let retire = binding(
                &mut release,
                None,
                Value::Runtime {
                    op: RuntimeOp::Retire,
                    args: vec![input.into(), Const::String("Read42".into()).into()],
                },
            );
            release.body = Some(release.seqs.alloc(Seq {
                stmts: vec![retire],
                term: Term::Return(None),
            }));
            let release = p.functions.alloc(release);
            let mut f = simple();
            f.suspends = suspends;
            let op = reference(&mut f, ft);
            let h = reference(&mut f, ht);
            let mut statements = vec![
                binding(&mut f, Some(op), Value::FnRef(callback)),
                binding(
                    &mut f,
                    Some(h),
                    Value::Handler {
                        name: "Read42".into(),
                        effect,
                        ops: vec![op],
                    },
                ),
            ];
            let alias = if kind == "direct" {
                h
            } else {
                let alias = reference(&mut f, ht);
                match kind {
                    "evidence" => {
                        let ev = reference(&mut f, TypeId::EVIDENCE);
                        statements.push(binding(
                            &mut f,
                            Some(ev),
                            Value::EvExtend {
                                base: None,
                                bindings: vec![(effect, h)],
                            },
                        ));
                        statements.push(binding(
                            &mut f,
                            Some(alias),
                            Value::EvLookup { ev, effect },
                        ));
                    }
                    "coerce" => {
                        let erased = reference(&mut f, TypeId::ANY);
                        statements.push(binding(&mut f, Some(erased), Value::Coerce(h)));
                        statements.push(binding(&mut f, Some(alias), Value::Coerce(erased)));
                    }
                    "tuple" => {
                        let ty = p.core.types.intern(Type::Tuple(vec![ht]));
                        let tuple = reference(&mut f, ty);
                        statements.push(binding(&mut f, Some(tuple), Value::Tuple(vec![h.into()])));
                        statements.push(binding(
                            &mut f,
                            Some(alias),
                            Value::TupleGet {
                                base: tuple,
                                index: 0,
                            },
                        ));
                    }
                    "argument" => statements.push(binding(
                        &mut f,
                        Some(alias),
                        Value::Call {
                            callee: Callee::Direct(identity),
                            args: vec![h.into()],
                            suspends: false,
                            on_fail: None,
                        },
                    )),
                    _ => unreachable!(),
                }
                alias
            };
            statements.push(binding(
                &mut f,
                None,
                Value::Call {
                    callee: Callee::Direct(release),
                    args: vec![alias.into()],
                    suspends: false,
                    on_fail: None,
                },
            ));
            let result = variable(&mut f, TypeId::INT);
            statements.push(binding(
                &mut f,
                Some(result),
                Value::Call {
                    callee: Callee::Op {
                        handler: h,
                        effect,
                        op: 0,
                    },
                    args: vec![],
                    suspends: false,
                    on_fail: None,
                },
            ));
            f.seqs[f.body.unwrap()] = Seq {
                stmts: statements,
                term: Term::Return(Some(result.into())),
            };
            p.functions.alloc(f);
            let output =
                crate::backend::compile_program(p, &Options::default().with_heap_bytes(2048))
                    .unwrap();
            let invoke = if suspends {
                "int32_t result=0;(void)answer_start(&result);while(emela_scheduler_run(1)){}"
            } else {
                "(void)answer();"
            };
            let harness = format!(
                "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{const char expected[]=\"handler Read42 was used after release\";exit(k==EMELA_DEFECT_PANIC&&n==sizeof(expected)-1&&!memcmp(m,expected,n)?0:2);}}\nint main(void){{{invoke}return 1;}}\n"
            );
            execute(&output, &harness, 0);
        }
    }
}
