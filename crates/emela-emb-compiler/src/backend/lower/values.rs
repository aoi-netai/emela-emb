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
        Value::Call {
            callee: Callee::Direct(id),
            args,
            suspends: false,
            on_fail: None,
        } => {
            let target = program
                .functions
                .iter()
                .find_map(|(key, value)| (key == *id).then_some(value))
                .ok_or_else(|| invalid(&f.name, "callee ID is out of bounds"))?;
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
        _ => Err(unsupported(&f.name, "value variant")),
    }
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
