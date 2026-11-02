use super::*;

pub(super) fn value_type(
    program: &Program,
    f: &Function,
    value: &Value,
    available: &HashSet<VarId>,
    expected: Option<TypeId>,
) -> Result<TypeId> {
    match value {
        Value::Const(value) => operand(f, &Operand::Const(value.clone()), available),
        Value::Closure(closure) => {
            let ty = expected
                .ok_or_else(|| invalid(&f.name, "closure needs a concrete function destination"))?;
            let Type::Fn(ft) = program.core.ty(ty) else {
                return Err(invalid(
                    &f.name,
                    "closure destination is not a function value",
                ));
            };
            let mut params = function_parameters(ft);
            if ft.ev {
                params.push(TypeId::EVIDENCE);
            }
            if closure
                .params
                .iter()
                .map(|&p| variable(f, p))
                .collect::<Result<Vec<_>>>()?
                != params
                || closure.suspends != ft.suspends
            {
                return Err(invalid(
                    &f.name,
                    "closure parameters differ from its function type",
                ));
            }
            let mut bindings = HashSet::new();
            for &param in &closure.params {
                if !bindings.insert(param) {
                    return Err(invalid(&f.name, "closure has a duplicate parameter"));
                }
            }
            for &capture in &closure.captures {
                operand(f, &Operand::Var(capture), available)?;
                if !bindings.insert(capture) {
                    return Err(invalid(
                        &f.name,
                        "closure has a duplicate or parameter capture",
                    ));
                }
            }
            Ok(ty)
        }
        Value::FnRef(id) => {
            let target = program
                .functions
                .iter()
                .find_map(|(key, value)| (key == *id).then_some(value))
                .ok_or_else(|| invalid(&f.name, "function reference ID is out of bounds"))?;
            let Some(expected) = expected else {
                return Ok(TypeId::UNIT);
            };
            let Type::Fn(ft) = program.core.ty(expected) else {
                return Err(invalid(
                    &f.name,
                    "function reference destination is not a function value",
                ));
            };
            let mut params = function_parameters(ft);
            if ft.ev {
                params.push(TypeId::EVIDENCE);
            }
            if target
                .params
                .iter()
                .map(|&p| variable(target, p))
                .collect::<Result<Vec<_>>>()?
                != params
                || target.ret != ft.ret
                || target.fails != ft.fails
                || target.suspends != ft.suspends
            {
                return Err(invalid(
                    &f.name,
                    "function reference type differs from its target ABI",
                ));
            }
            Ok(expected)
        }
        Value::Call {
            callee: Callee::Indirect(callee),
            args,
            suspends,
            on_fail,
        } => {
            let ty = operand(f, &Operand::Var(*callee), available)?;
            let Type::Fn(ft) = program.core.ty(ty) else {
                return Err(invalid(
                    &f.name,
                    "indirect call target is not a function value",
                ));
            };
            if ft.fails != on_fail.is_some() || ft.suspends != *suspends {
                return Err(invalid(
                    &f.name,
                    "indirect call channels differ from its function type",
                ));
            }
            if *suspends && !f.suspends {
                return Err(invalid(
                    &f.name,
                    "suspending indirect call requires a suspending caller",
                ));
            }
            let mut params = function_parameters(ft);
            if ft.ev {
                params.push(TypeId::EVIDENCE);
            }
            if args.len() != params.len() {
                return Err(invalid(
                    &f.name,
                    "indirect call argument count differs from its function type",
                ));
            }
            for (argument, expected) in args.iter().zip(params) {
                expect_type(f, operand(f, argument, available)?, expected)?;
            }
            Ok(ft.ret)
        }
        Value::Call {
            callee:
                Callee::Op {
                    handler,
                    effect,
                    op,
                },
            args,
            suspends,
            on_fail,
        } => {
            expect_type(
                f,
                operand(f, &Operand::Var(*handler), available)?,
                handler_type(program, f, *effect)?,
            )?;
            let operation = effect_definition(program, f, *effect)?
                .ops
                .get(*op)
                .ok_or_else(|| invalid(&f.name, "effect operation index is out of bounds"))?;
            if operation.type_params != 0 {
                return Err(unsupported(
                    &f.name,
                    "unspecialized generic effect operation",
                ));
            }
            if operation.fails != on_fail.is_some() || (!operation.suspend && *suspends) {
                return Err(invalid(
                    &f.name,
                    "operation call channels differ from its declaration",
                ));
            }
            let parameters: Vec<_> = operation
                .params
                .iter()
                .copied()
                .filter(|ty| !matches!(*ty, TypeId::UNIT | TypeId::NEVER))
                .collect();
            arguments(f, args, &parameters, available)?;
            Ok(operation.ret)
        }
        Value::Call {
            callee: Callee::Direct(id),
            args,
            suspends,
            on_fail,
        } => {
            let target = program
                .functions
                .iter()
                .find_map(|(key, value)| (key == *id).then_some(value))
                .ok_or_else(|| invalid(&f.name, "callee ID is out of bounds"))?;
            if target.fails != on_fail.is_some() {
                return Err(invalid(
                    &f.name,
                    "call failure edge differs from callee failure channel",
                ));
            }
            if target.suspends != *suspends {
                return Err(invalid(
                    &f.name,
                    "call suspension edge differs from callee suspension channel",
                ));
            }
            if *suspends && !f.suspends {
                return Err(invalid(
                    &f.name,
                    "suspending direct call requires a suspending caller",
                ));
            }
            if args.len() != target.params.len() {
                return Err(invalid(
                    &f.name,
                    "call argument count does not match callee parameters",
                ));
            }
            for (arg, &param) in args.iter().zip(&target.params) {
                expect_type(f, operand(f, arg, available)?, variable(target, param)?)?;
            }
            Ok(target.ret)
        }
        Value::Handler { effect, ops, .. } => {
            let definition = effect_definition(program, f, *effect)?;
            if definition.ops.len() != ops.len() {
                return Err(invalid(
                    &f.name,
                    "handler operation count differs from its declaration",
                ));
            }
            for (&operation, declared) in ops.iter().zip(&definition.ops) {
                let ty = operand(f, &Operand::Var(operation), available)?;
                let Type::Fn(ft) = program.core.ty(ty) else {
                    return Err(invalid(
                        &f.name,
                        "handler operation is not a function value",
                    ));
                };
                if declared.type_params != 0 {
                    return Err(unsupported(&f.name, "generic handler operation"));
                }
                let mut parameters: Vec<_> = declared
                    .params
                    .iter()
                    .copied()
                    .filter(|ty| !matches!(*ty, TypeId::UNIT | TypeId::NEVER))
                    .collect();
                let ret = if declared.suspend {
                    let Some(&resume) = ft.params.last() else {
                        return Err(invalid(
                            &f.name,
                            "suspend handler operation is missing its resume parameter",
                        ));
                    };
                    let valid = matches!(program.core.ty(resume), Type::Fn(resume)
                        if resume.params == [declared.ret] && resume.ret == TypeId::UNIT
                            && !resume.fails && !resume.suspends && !resume.ev && resume.sets.is_none());
                    if !valid {
                        return Err(invalid(
                            &f.name,
                            "suspend handler operation has an invalid resume function type",
                        ));
                    }
                    parameters.push(resume);
                    TypeId::UNIT
                } else {
                    declared.ret
                };
                if function_parameters(ft) != parameters
                    || ft.ret != ret
                    || ft.fails != declared.fails
                    || (!declared.suspend && ft.suspends)
                    || ft.ev
                    || ft.sets.is_some()
                {
                    return Err(invalid(
                        &f.name,
                        "handler operation type differs from its declaration",
                    ));
                }
            }
            handler_type(program, f, *effect)
        }
        Value::EvExtend { base, bindings } => {
            if let Some(base) = base {
                expect_type(
                    f,
                    operand(f, &Operand::Var(*base), available)?,
                    TypeId::EVIDENCE,
                )?;
            }
            for &(effect, handler) in bindings {
                expect_type(
                    f,
                    operand(f, &Operand::Var(handler), available)?,
                    handler_type(program, f, effect)?,
                )?;
            }
            Ok(TypeId::EVIDENCE)
        }
        Value::EvLookup { ev, effect } => {
            expect_type(
                f,
                operand(f, &Operand::Var(*ev), available)?,
                TypeId::EVIDENCE,
            )?;
            handler_type(program, f, *effect)
        }
        Value::Prim { op, ty, args } => {
            let ty = match ty {
                PrimTy::I32 => TypeId::INT,
                PrimTy::I64 => TypeId::INT64,
                PrimTy::F64 => TypeId::FLOAT,
                PrimTy::String => TypeId::STRING,
            };
            let unary = matches!(op, PrimOp::Neg | PrimOp::Not);
            if args.len() != if unary { 1 } else { 2 } {
                return Err(invalid(&f.name, "primitive argument count is invalid"));
            }
            let mut first_type = None;
            for arg in args {
                let actual = operand(f, arg, available)?;
                if first_type.is_some_and(|first| first != actual) {
                    return Err(invalid(&f.name, "primitive operands have different types"));
                }
                first_type = Some(actual);
                // Bool comparisons use the I32 primitive representation.
                if !(ty == TypeId::INT
                    && actual == TypeId::BOOL
                    && matches!(op, PrimOp::Not | PrimOp::Eq | PrimOp::Ne))
                {
                    expect_type(f, actual, ty)?;
                }
            }
            Ok(
                if matches!(
                    op,
                    PrimOp::Not
                        | PrimOp::Eq
                        | PrimOp::Ne
                        | PrimOp::Lt
                        | PrimOp::Le
                        | PrimOp::Gt
                        | PrimOp::Ge
                ) {
                    TypeId::BOOL
                } else {
                    ty
                },
            )
        }
        Value::Tuple(args) => {
            let fields = args
                .iter()
                .map(|value| operand(f, value, available))
                .collect::<Result<Vec<_>>>()?;
            if let Some(expected) = expected {
                if program.core.ty(expected) == &Type::Tuple(fields.clone()) {
                    return Ok(expected);
                }
            }
            program
                .core
                .types
                .ids
                .get(&Type::Tuple(fields))
                .copied()
                .ok_or_else(|| {
                    invalid(
                        &f.name,
                        "tuple result type is missing from the type catalog",
                    )
                })
        }
        Value::TupleGet { base, index } => {
            let ty = operand(f, &Operand::Var(*base), available)?;
            let Type::Tuple(fields) = program.core.ty(ty) else {
                return Err(invalid(
                    &f.name,
                    "tuple projection base has a different type",
                ));
            };
            fields
                .get(*index)
                .copied()
                .ok_or_else(|| invalid(&f.name, "tuple projection index is out of bounds"))
        }
        Value::Make { ctor, args } => {
            let ty = expected
                .or_else(|| {
                    program
                        .core
                        .types
                        .ids
                        .get(&Type::Enum(ctor.enum_id, vec![]))
                        .copied()
                })
                .ok_or_else(|| {
                    invalid(
                        &f.name,
                        "enum constructor needs a concrete destination type",
                    )
                })?;
            if !matches!(program.core.ty(ty),Type::Enum(id,_) if *id == ctor.enum_id) {
                return Err(invalid(
                    &f.name,
                    "constructor enum differs from destination enum",
                ));
            }
            arguments(
                f,
                args,
                &types::enum_fields(program, &f.name, ty, ctor.variant)?,
                available,
            )?;
            Ok(ty)
        }
        Value::Field { base, ctor, index } => {
            let ty = operand(f, &Operand::Var(*base), available)?;
            if !matches!(program.core.ty(ty),Type::Enum(id,_) if *id == ctor.enum_id) {
                return Err(invalid(
                    &f.name,
                    "field constructor enum differs from base enum",
                ));
            }
            types::enum_fields(program, &f.name, ty, ctor.variant)?
                .get(*index)
                .copied()
                .ok_or_else(|| invalid(&f.name, "enum field index is out of bounds"))
        }
        Value::Tag(base) => {
            let ty = operand(f, &Operand::Var(*base), available)?;
            if !matches!(program.core.ty(ty), Type::Enum(_, _)) {
                return Err(invalid(&f.name, "Tag base is not an enum"));
            }
            Ok(TypeId::INT)
        }
        Value::List { elems, tail } => {
            let inferred = match (expected, tail, elems.first()) {
                (Some(ty), _, _) => Some(ty),
                (_, Some(tail), _) => Some(operand(f, tail, available)?),
                (_, _, Some(first)) => Some(list_type(program, f, operand(f, first, available)?)?),
                _ => None,
            };
            let ty = inferred
                .ok_or_else(|| invalid(&f.name, "empty List needs a concrete destination type"))?;
            let Type::List(element) = program.core.ty(ty) else {
                return Err(invalid(&f.name, "List destination is not a List"));
            };
            for item in elems {
                expect_type(f, operand(f, item, available)?, *element)?;
            }
            if let Some(tail) = tail {
                expect_type(f, operand(f, tail, available)?, ty)?;
            }
            Ok(ty)
        }
        Value::ListIsEmpty(base) | Value::ListHead(base) | Value::ListTail(base) => {
            let ty = operand(f, &Operand::Var(*base), available)?;
            let Type::List(element) = program.core.ty(ty) else {
                return Err(invalid(&f.name, "List operation base is not a List"));
            };
            Ok(match value {
                Value::ListIsEmpty(_) => TypeId::BOOL,
                Value::ListHead(_) => *element,
                _ => ty,
            })
        }
        Value::Coerce(base) => {
            let source = operand(f, &Operand::Var(*base), available)?;
            let destination = expected.unwrap_or(source);
            if let (Type::Fn(from), Type::Fn(to)) =
                (program.core.ty(source), program.core.ty(destination))
            {
                if from.params != to.params
                    || (from.ret != to.ret && from.ret != TypeId::NEVER)
                    || (from.suspends && !to.suspends)
                    || (from.ev && !to.ev)
                    || from.sets.is_some()
                    || to.sets.is_some()
                {
                    return Err(invalid(
                        &f.name,
                        "function coercion has incompatible parameters, result, or effect flags",
                    ));
                }
                return Ok(destination);
            }
            if source != destination && source != TypeId::ANY && destination != TypeId::ANY {
                return Err(invalid(
                    &f.name,
                    "data coercion requires identical types or Any",
                ));
            }
            Ok(destination)
        }
        Value::IsError {
            error,
            enum_id,
            error_type,
        }
        | Value::AsError {
            error,
            enum_id,
            error_type,
        } => {
            expect_type(
                f,
                operand(f, &Operand::Var(*error), available)?,
                TypeId::ANY,
            )?;
            if !program
                .core
                .enums
                .iter()
                .any(|(id, definition)| id == *enum_id && definition.error)
            {
                return Err(invalid(
                    &f.name,
                    "error operation refers to a non-error enum",
                ));
            }
            if !matches!(program.core.types.types.get(error_type.index()), Some(Type::Enum(id, _)) if id == enum_id)
            {
                return Err(invalid(
                    &f.name,
                    "error operation type differs from its error enum",
                ));
            }
            types::validate_type(program, &f.name, *error_type)?;
            if matches!(value, Value::IsError { .. }) {
                return Ok(TypeId::BOOL);
            }
            let ty = *error_type;
            if !matches!(program.core.ty(ty),Type::Enum(id,_) if *id == *enum_id) {
                return Err(invalid(
                    &f.name,
                    "error projection destination has a different enum type",
                ));
            }
            Ok(ty)
        }
        Value::Builtin { op, args } => builtin_type(program, f, *op, args, available, expected),
        Value::Runtime {
            op: RuntimeOp::ShowString,
            args,
        } => {
            arguments(f, args, &[TypeId::STRING], available)?;
            Ok(TypeId::STRING)
        }
        Value::Runtime {
            op: RuntimeOp::DebugPrint,
            args,
        } => {
            arguments(f, args, &[TypeId::STRING], available)?;
            Ok(TypeId::UNIT)
        }
        Value::Runtime {
            op: RuntimeOp::Retire,
            args,
        } => {
            if args.len() != 2 || expected.is_some() {
                return Err(invalid(
                    &f.name,
                    "Retire needs a handler and name with no destination",
                ));
            }
            let Operand::Var(handler) = args[0] else {
                return Err(invalid(&f.name, "Retire handler is not a variable"));
            };
            let ty = operand(f, &Operand::Var(handler), available)?;
            if !matches!(program.core.ty(ty), Type::Handler(_)) {
                return Err(invalid(&f.name, "Retire value is not a handler"));
            }
            expect_type(f, operand(f, &args[1], available)?, TypeId::STRING)?;
            Ok(TypeId::UNIT)
        }
        _ => Err(unsupported(&f.name, "value variant")),
    }
}

fn function_parameters(ft: &FnType) -> Vec<TypeId> {
    ft.params
        .iter()
        .copied()
        .filter(|ty| !matches!(*ty, TypeId::UNIT | TypeId::NEVER))
        .collect()
}

fn effect_definition<'a>(
    program: &'a Program,
    f: &Function,
    effect: EffectId,
) -> Result<&'a EffectDef> {
    program
        .core
        .effects
        .iter()
        .find_map(|(id, definition)| (id == effect).then_some(definition))
        .ok_or_else(|| invalid(&f.name, "effect ID is out of bounds"))
}

fn handler_type(program: &Program, f: &Function, effect: EffectId) -> Result<TypeId> {
    effect_definition(program, f, effect)?;
    let effect = crate::semantics::effect_base(&program.core, effect)
        .ok_or_else(|| invalid(&f.name, "handler effect label has an invalid base"))?;
    program
        .core
        .types
        .ids
        .get(&Type::Handler(effect))
        .copied()
        .ok_or_else(|| invalid(&f.name, "handler type is missing from the type catalog"))
}

fn arguments(
    f: &Function,
    args: &[Operand],
    expected: &[TypeId],
    available: &HashSet<VarId>,
) -> Result<()> {
    if args.len() != expected.len() {
        return Err(invalid(&f.name, "builtin argument count is invalid"));
    }
    for (arg, &ty) in args.iter().zip(expected) {
        expect_type(f, operand(f, arg, available)?, ty)?;
    }
    Ok(())
}
fn builtin_type(
    program: &Program,
    f: &Function,
    op: Builtin,
    args: &[Operand],
    available: &HashSet<VarId>,
    expected: Option<TypeId>,
) -> Result<TypeId> {
    use Builtin::*;
    let (inputs, result) = match op {
        CellNew => {
            if args.len() != 1 {
                return Err(invalid(&f.name, "CellNew needs one initial value"));
            }
            let content = operand(f, &args[0], available)?;
            let result = program
                .core
                .types
                .ids
                .get(&Type::Cell(content))
                .copied()
                .ok_or_else(|| invalid(&f.name, "Cell type is missing from the type catalog"))?;
            (vec![content], result)
        }
        CellGet | CellSet => {
            let Some(cell) = args.first() else {
                return Err(invalid(&f.name, "Cell operation needs a cell argument"));
            };
            let cell_type = operand(f, cell, available)?;
            let Type::Cell(content) = program.core.ty(cell_type) else {
                return Err(invalid(&f.name, "Cell operation base is not a Cell"));
            };
            if op == CellGet {
                (vec![cell_type], *content)
            } else {
                (vec![cell_type, *content], TypeId::UNIT)
            }
        }
        IntBitNot => (vec![TypeId::INT], TypeId::INT),
        IntBitAnd | IntBitOr | IntBitXor | IntShiftLeft | IntShiftRight | IntShiftRightUnsigned => {
            (vec![TypeId::INT; 2], TypeId::INT)
        }
        Int64BitNot => (vec![TypeId::INT64], TypeId::INT64),
        Int64BitAnd | Int64BitOr | Int64BitXor => (vec![TypeId::INT64; 2], TypeId::INT64),
        Int64ShiftLeft | Int64ShiftRight | Int64ShiftRightUnsigned => {
            (vec![TypeId::INT64, TypeId::INT], TypeId::INT64)
        }
        Int64FromInt => (vec![TypeId::INT], TypeId::INT64),
        Int64ToInt => (vec![TypeId::INT64], TypeId::INT),
        FloatFromInt => (vec![TypeId::INT], TypeId::FLOAT),
        FloatFloor | FloatCeil | FloatRound | FloatTruncate => (vec![TypeId::FLOAT], TypeId::INT),
        IntToString => (vec![TypeId::INT], TypeId::STRING),
        Int64ToString => (vec![TypeId::INT64], TypeId::STRING),
        FloatToString => (vec![TypeId::FLOAT], TypeId::STRING),
        StringLength | StringByteSize => (vec![TypeId::STRING], TypeId::INT),
        StringAppend => (vec![TypeId::STRING; 2], TypeId::STRING),
        StringContains | StringStartsWith | StringEndsWith => {
            (vec![TypeId::STRING; 2], TypeId::BOOL)
        }
        StringTrim => (vec![TypeId::STRING], TypeId::STRING),
        StringSplit => (
            vec![TypeId::STRING; 2],
            list_type(program, f, TypeId::STRING)?,
        ),
        StringChars => (vec![TypeId::STRING], list_type(program, f, TypeId::STRING)?),
        StringCodePoints => (vec![TypeId::STRING], list_type(program, f, TypeId::INT)?),
        StringJoin => (
            vec![list_type(program, f, TypeId::STRING)?, TypeId::STRING],
            TypeId::STRING,
        ),
        BytesToList => (vec![TypeId::BYTES], list_type(program, f, TypeId::INT)?),
        BytesSize => (vec![TypeId::BYTES], TypeId::INT),
        BytesSlice => (vec![TypeId::BYTES, TypeId::INT, TypeId::INT], TypeId::BYTES),
        BytesConcat => (vec![list_type(program, f, TypeId::BYTES)?], TypeId::BYTES),
        BytesEncodeUtf8 => (vec![TypeId::STRING], TypeId::BYTES),
        BytesDecodeValidUtf8 => (vec![TypeId::BYTES], TypeId::STRING),
        BytesFromList | BytesGet => {
            let (inputs, payload) = match op {
                BytesFromList => (vec![list_type(program, f, TypeId::INT)?], TypeId::BYTES),
                _ => (vec![TypeId::BYTES, TypeId::INT], TypeId::INT),
            };
            arguments(f, args, &inputs, available)?;
            return types::option_type(program, &f.name, payload, expected);
        }
        IntParse | FloatParse | IntCheckedAdd | StringFromCodePoint => {
            let (inputs, payload) = match op {
                IntParse => (vec![TypeId::STRING], TypeId::INT),
                FloatParse => (vec![TypeId::STRING], TypeId::FLOAT),
                IntCheckedAdd => (vec![TypeId::INT; 2], TypeId::INT),
                _ => (vec![TypeId::INT], TypeId::STRING),
            };
            arguments(f, args, &inputs, available)?;
            return types::option_type(program, &f.name, payload, expected);
        }
        _ => return Err(unsupported(&f.name, "builtin")),
    };
    arguments(f, args, &inputs, available)?;
    Ok(result)
}

fn list_type(program: &Program, f: &Function, element: TypeId) -> Result<TypeId> {
    program
        .core
        .types
        .ids
        .get(&Type::List(element))
        .copied()
        .ok_or_else(|| invalid(&f.name, "List type is missing from the type catalog"))
}
