use super::*;

fn empty_effect(p: &mut Program) -> (EffectId, TypeId) {
    let effect = p.core.effects.alloc(EffectDef {
        name: "Empty".into(),
        decl: "Empty".into(),
        args: vec![],
        ops: vec![],
        label_of: None,
    });
    let handler = p.core.types.intern(Type::Handler(effect));
    (effect, handler)
}

#[test]
fn discarded_handler_and_evidence_constructors_release_before_the_next_statement() {
    for kind in ["handler", "extend", "chars"] {
        let construct_handler = kind == "handler";
        let mut p = program(vec![]);
        let (effect, ht) = empty_effect(&mut p);
        p.core.types.intern(Type::List(TypeId::STRING));
        let mut inspect = Function::new("inspect_live");
        inspect.external = Some(External {
            bindings: vec![ExternalBinding {
                target: "c".into(),
                host: HostRef::Global("inspect_live".into()),
            }],
        });
        let inspect = p.functions.alloc(inspect);
        let mut f = simple();
        let h = aggregate_variable(&mut f, ht);
        let make = binding(
            &mut f,
            Some(h),
            Value::Handler {
                name: "Empty".into(),
                effect,
                ops: vec![],
            },
        );
        let left = variable(&mut f, TypeId::INT);
        let zero = variable(&mut f, TypeId::BOOL);
        let less = variable(&mut f, TypeId::INT);
        let value = if kind == "chars" {
            Value::Builtin {
                op: Builtin::StringChars,
                args: vec![Const::String("aβ".into()).into()],
            }
        } else if construct_handler {
            Value::Handler {
                name: "Empty".into(),
                effect,
                ops: vec![],
            }
        } else {
            Value::EvExtend {
                base: None,
                bindings: vec![(effect, h)],
            }
        };
        let discard = binding(&mut f, None, value);
        let check = binding(
            &mut f,
            None,
            Value::Call {
                callee: Callee::Direct(inspect),
                args: vec![],
                suspends: false,
                on_fail: None,
            },
        );
        let is_zero = binding(
            &mut f,
            Some(zero),
            Value::Prim {
                op: PrimOp::Eq,
                ty: PrimTy::I32,
                args: vec![left.into(), Const::Int(0).into()],
            },
        );
        let done = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Return(Some(Const::Int(42).into())),
        });
        let more = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Yield(None),
        });
        let choose = f.stmts.alloc(Stmt::If {
            dst: None,
            cond: zero.into(),
            then: done,
            else_: more,
        });
        let subtract = binding(
            &mut f,
            Some(less),
            Value::Prim {
                op: PrimOp::Sub,
                ty: PrimTy::I32,
                args: vec![left.into(), Const::Int(1).into()],
            },
        );
        let body = f.seqs.alloc(Seq {
            stmts: vec![discard, check, is_zero, choose, subtract],
            term: Term::Recur(vec![less.into()]),
        });
        let loop_ = f.stmts.alloc(Stmt::Loop {
            dst: None,
            vars: vec![left],
            inits: vec![Const::Int(999).into()],
            body,
        });
        f.seqs[f.body.unwrap()].stmts = vec![make, loop_];
        p.functions.alloc(f);
        let mut output =
            crate::backend::compile_program(p, &Options::default().with_heap_bytes(1024)).unwrap();
        audit_owned_heap(&mut output);
        output.files.iter_mut().find(|file|file.path=="emela_embedded.c").unwrap().contents.push_str("\nvoid inspect_live(void){int live=0;for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)live++;p+=b->size;}if(live!=EXPECTED)emela_board_defect(EMELA_DEFECT_PANIC,\"x\",1);}\n");
        let file = output
            .files
            .iter_mut()
            .find(|file| file.path == "emela_embedded.c")
            .unwrap();
        file.contents = file.contents.replace(
            "live!=EXPECTED",
            if kind != "extend" {
                "live!=0"
            } else {
                "live!=1"
            },
        );
        execute(
            &output,
            "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
            0,
        );
    }
}

#[test]
fn discarded_dynamic_evidence_lookup_checks_missing_handlers_without_releasing_the_caller() {
    for discard in [false, true] {
        let mut p = program(vec![]);
        let (effect, ht) = empty_effect(&mut p);
        let mut make = Function::new("make");
        make.exported = true;
        make.export_symbol = Some("make".into());
        make.ret = TypeId::EVIDENCE;
        let h = aggregate_variable(&mut make, ht);
        let ev = aggregate_variable(&mut make, TypeId::EVIDENCE);
        let handler = binding(
            &mut make,
            Some(h),
            Value::Handler {
                name: "Empty".into(),
                effect,
                ops: vec![],
            },
        );
        let extend = binding(
            &mut make,
            Some(ev),
            Value::EvExtend {
                base: None,
                bindings: vec![(effect, h)],
            },
        );
        make.body = Some(make.seqs.alloc(Seq {
            stmts: vec![handler, extend],
            term: Term::Return(Some(ev.into())),
        }));
        p.functions.alloc(make);
        let mut run = Function::new("run");
        run.exported = true;
        run.export_symbol = Some("run".into());
        let ev = aggregate_variable(&mut run, TypeId::EVIDENCE);
        run.params.push(ev);
        let result = if discard {
            None
        } else {
            Some(aggregate_variable(&mut run, ht))
        };
        let lookup = binding(&mut run, result, Value::EvLookup { ev, effect });
        run.body = Some(run.seqs.alloc(Seq {
            stmts: vec![lookup],
            term: Term::Return(None),
        }));
        p.functions.alloc(run);
        let mut output =
            crate::backend::compile_program(p, &Options::default().with_heap_bytes(1024)).unwrap();
        audit_owned_heap(&mut output);
        execute(
            &output,
            "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){exit(k==EMELA_DEFECT_PANIC&&n==23&&!memcmp(m,\"effect handler not found\",23)?0:2);}\nint main(void){for(int i=0;i<1000;i++){void*ev=make();for(int n=0;n<13;n++)run(ev);emela_embedded_release(ev);if(!test_heap_empty())return 3;}run(NULL);return 1;}\n",
            0,
        );
    }
}
