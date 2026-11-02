use super::*;

fn reference(f: &mut Function, ty: TypeId) -> VarId {
    f.vars.alloc(VarData {
        hint: String::new(),
        ty,
        repr: Repr::Ref,
    })
}

fn read_effect(p: &mut Program) -> (EffectId, TypeId, TypeId) {
    let effect = p.core.effects.alloc(EffectDef {
        name: "Read".into(),
        decl: "Read".into(),
        args: vec![],
        label_of: None,
        ops: vec![OpDef {
            name: "read".into(),
            suspend: false,
            type_params: 0,
            params: vec![],
            ret: TypeId::INT,
            fails: false,
        }],
    });
    let handler = p.core.types.intern(Type::Handler(effect));
    let operation = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    (effect, handler, operation)
}

fn callback(p: &mut Program) -> FnId {
    let mut function = Function::new("read_42");
    function.ret = TypeId::INT;
    function.body = Some(function.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    }));
    p.functions.alloc(function)
}

fn audit_heap(output: &mut Output) {
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
}

#[test]
fn static_evidence_resolves_without_heap_and_invalid_handler_operations_are_rejected() {
    let mut p = program(vec![]);
    let (effect, ht, ft) = read_effect(&mut p);
    let callback = callback(&mut p);
    let mut f = simple();
    let op = reference(&mut f, ft);
    let handler = reference(&mut f, ht);
    let evidence = reference(&mut f, TypeId::EVIDENCE);
    let looked_up = reference(&mut f, ht);
    let result = variable(&mut f, TypeId::INT);
    let operation = binding(&mut f, Some(op), Value::FnRef(callback));
    let make = binding(
        &mut f,
        Some(handler),
        Value::Handler {
            name: "Read42".into(),
            effect,
            ops: vec![op],
        },
    );
    let extend = binding(
        &mut f,
        Some(evidence),
        Value::EvExtend {
            base: None,
            bindings: vec![(effect, handler)],
        },
    );
    let lookup = binding(
        &mut f,
        Some(looked_up),
        Value::EvLookup {
            ev: evidence,
            effect,
        },
    );
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Op {
                handler: looked_up,
                effect,
                op: 0,
            },
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![operation, make, extend, lookup, call],
        term: Term::Return(Some(result.into())),
    };
    let fid = p.functions.alloc(f);
    let output = crate::backend::compile_program(p.clone(), &Options::default()).unwrap();
    execute(&output, DEFECT_HARNESS, 0);
    assert!(!output.c_source.contains("emela_evidence_node"));
    assert!(!output.c_source.contains("emela_heap_allocate("));
    let mut bad = p.clone();
    if let Stmt::Let {
        value: Value::Handler { ops, .. },
        ..
    } = &mut bad.functions[fid].stmts[make]
    {
        ops.clear();
    }
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("handler operation count")
    );
    let mut bad = p;
    bad.core.effects = Default::default();
    assert!(
        lower_program(bad).unwrap_err()[0]
            .message
            .contains("effect ID is out of bounds")
    );
}

#[test]
fn dynamic_evidence_keeps_handlers_alive_and_protection_retires_and_releases_them() {
    let mut p = program(vec![]);
    let (effect, ht, ft) = read_effect(&mut p);
    let callback = callback(&mut p);
    let mut invoke = Function::new("invoke_evidence");
    invoke.ret = TypeId::INT;
    let ev = reference(&mut invoke, TypeId::EVIDENCE);
    invoke.params.push(ev);
    let looked_up = reference(&mut invoke, ht);
    let result = variable(&mut invoke, TypeId::INT);
    let lookup = binding(&mut invoke, Some(looked_up), Value::EvLookup { ev, effect });
    let call = binding(
        &mut invoke,
        Some(result),
        Value::Call {
            callee: Callee::Op {
                handler: looked_up,
                effect,
                op: 0,
            },
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    invoke.body = Some(invoke.seqs.alloc(Seq {
        stmts: vec![lookup, call],
        term: Term::Return(Some(result.into())),
    }));
    let invoke = p.functions.alloc(invoke);
    let mut mark = Function::new("mark_release");
    mark.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("mark_release".into()),
        }],
    });
    let mark = p.functions.alloc(mark);
    let mut f = simple();
    let op = reference(&mut f, ft);
    let handler = reference(&mut f, ht);
    let evidence = reference(&mut f, TypeId::EVIDENCE);
    let result = variable(&mut f, TypeId::INT);
    let operation = binding(&mut f, Some(op), Value::FnRef(callback));
    let make = binding(
        &mut f,
        Some(handler),
        Value::Handler {
            name: "Read42".into(),
            effect,
            ops: vec![op],
        },
    );
    let extend = binding(
        &mut f,
        Some(evidence),
        Value::EvExtend {
            base: None,
            bindings: vec![(effect, handler)],
        },
    );
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Direct(invoke),
            args: vec![evidence.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let body = f.seqs.alloc(Seq {
        stmts: vec![call],
        term: Term::Yield(Some(result.into())),
    });
    let retire = binding(
        &mut f,
        None,
        Value::Runtime {
            op: RuntimeOp::Retire,
            args: vec![handler.into(), Const::String("Read42".into()).into()],
        },
    );
    let mark = binding(
        &mut f,
        None,
        Value::Call {
            callee: Callee::Direct(mark),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let cleanup = f.seqs.alloc(Seq {
        stmts: vec![retire, mark],
        term: Term::Yield(None),
    });
    let protect = f.stmts.alloc(Stmt::Protect {
        dst: Some(result),
        body,
        cleanup,
    });
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![operation, make, extend, protect],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int releases;\nvoid mark_release(void){releases++;}\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){if(answer()!=42||releases!=i+1||!test_heap_empty())return 1;}return 0;}\n",
        0,
    );
}

#[test]
fn cell_closure_any_cycles_are_collected_after_the_last_root_is_dropped() {
    let mut p = program(vec![]);
    let cell_type = p.core.types.intern(Type::Cell(TypeId::ANY));
    let ft = p.core.types.intern(Type::Fn(FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut f = simple();
    let zero = variable(&mut f, TypeId::INT);
    let initial = reference(&mut f, TypeId::ANY);
    let cell = reference(&mut f, cell_type);
    let closure = reference(&mut f, ft);
    let erased = reference(&mut f, TypeId::ANY);
    let loaded = reference(&mut f, TypeId::ANY);
    let restored = reference(&mut f, ft);
    let result = variable(&mut f, TypeId::INT);
    let initial_value = binding(&mut f, Some(zero), Value::Const(Const::Int(0)));
    let initial_binding = binding(&mut f, Some(initial), Value::Coerce(zero));
    let make_cell = binding(
        &mut f,
        Some(cell),
        Value::Builtin {
            op: Builtin::CellNew,
            args: vec![initial.into()],
        },
    );
    let body = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(Const::Int(42).into())),
    });
    let make_closure = binding(
        &mut f,
        Some(closure),
        Value::Closure(Box::new(Closure {
            params: vec![],
            captures: vec![cell],
            body,
            suspends: false,
        })),
    );
    let erase = binding(&mut f, Some(erased), Value::Coerce(closure));
    let store = binding(
        &mut f,
        None,
        Value::Builtin {
            op: Builtin::CellSet,
            args: vec![cell.into(), erased.into()],
        },
    );
    let load = binding(
        &mut f,
        Some(loaded),
        Value::Builtin {
            op: Builtin::CellGet,
            args: vec![cell.into()],
        },
    );
    let restore = binding(&mut f, Some(restored), Value::Coerce(loaded));
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Indirect(restored),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![
            initial_value,
            initial_binding,
            make_cell,
            make_closure,
            erase,
            store,
            load,
            restore,
            call,
        ],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let mut output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        0,
    );
}

#[test]
fn retirement_through_an_erased_alias_keeps_operation_active_checks() {
    let mut p = program(vec![]);
    let (effect, ht, ft) = read_effect(&mut p);
    let callback = callback(&mut p);
    let mut release = Function::new("release_read");
    let handler = reference(&mut release, ht);
    release.params.push(handler);
    let retire = binding(
        &mut release,
        None,
        Value::Runtime {
            op: RuntimeOp::Retire,
            args: vec![handler.into(), Const::String("Read42".into()).into()],
        },
    );
    release.body = Some(release.seqs.alloc(Seq {
        stmts: vec![retire],
        term: Term::Return(None),
    }));
    let release = p.functions.alloc(release);
    let mut f = simple();
    let op = reference(&mut f, ft);
    let handler = reference(&mut f, ht);
    let erased = reference(&mut f, TypeId::ANY);
    let alias = reference(&mut f, ht);
    let result = variable(&mut f, TypeId::INT);
    let operation = binding(&mut f, Some(op), Value::FnRef(callback));
    let make = binding(
        &mut f,
        Some(handler),
        Value::Handler {
            name: "Read42".into(),
            effect,
            ops: vec![op],
        },
    );
    let erase = binding(&mut f, Some(erased), Value::Coerce(handler));
    let restore = binding(&mut f, Some(alias), Value::Coerce(erased));
    let release = binding(
        &mut f,
        None,
        Value::Call {
            callee: Callee::Direct(release),
            args: vec![alias.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let call = binding(
        &mut f,
        Some(result),
        Value::Call {
            callee: Callee::Op {
                handler,
                effect,
                op: 0,
            },
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![operation, make, erase, restore, release, call],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char*expected=\"handler Read42 was used after release\";exit(k==EMELA_DEFECT_PANIC&&n==strlen(expected)&&memcmp(m,expected,n)==0?77:78);}\nint main(void){(void)answer();return 1;}\n",
        77,
    );
}
