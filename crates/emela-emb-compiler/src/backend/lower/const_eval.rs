//! Bounded static const evaluation over this compiler's owned typed IR.
use std::collections::HashMap;

use crate::contract::ir::Builtin;
use crate::contract::ir::{
    Callee, CaseKey, Const, EnumId, FnId, Function, Operand, PrimOp, PrimTy, Program as Module,
    Stmt, SwitchOn, Term, Type, Value, VarId,
};

use crate::semantics::unicode_grapheme::grapheme_count;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    Value(Const),
    Defect { kind: &'static str, message: String },
    SelfReference { message: String },
}

pub(crate) fn evaluate(module: &Module, function: FnId) -> Result<Outcome, &'static str> {
    evaluate_function(module, function, Vec::new(), &mut Vec::new(), &mut 4096)
}

fn evaluate_function(
    module: &Module,
    id: FnId,
    arguments: Vec<Const>,
    active: &mut Vec<FnId>,
    fuel: &mut usize,
) -> Result<Outcome, &'static str> {
    if *fuel == 0 || active.len() >= 64 {
        return Err("static const evaluation budget exhausted");
    }
    *fuel -= 1;
    let function = &module.functions[id];
    if active.contains(&id) {
        if function.constant {
            return Ok(Outcome::SelfReference {
                message: format!(
                    "const {} refers to itself while being evaluated",
                    function.name
                ),
            });
        }
        return Err("const evaluation reached a recursive helper function");
    }
    if arguments.len() != function.params.len() {
        return Err("direct call has an unsupported argument count");
    }
    if function.constant && !arguments.is_empty() {
        return Err("const function has parameters");
    }
    if function.fails || function.suspends || function.external.is_some() {
        return Err("called function has a failure, suspension, or external body");
    }
    let body = function.body.ok_or("called function has no body")?;

    active.push(id);
    let result = evaluate_body(module, function, body, arguments, active, fuel);
    active.pop();
    let outcome = result?;

    if function.constant {
        Ok(match outcome {
            Outcome::Defect { kind, message } => Outcome::Defect {
                kind,
                message: format!("while evaluating const {}: {message}", function.name),
            },
            other => other,
        })
    } else {
        Ok(outcome)
    }
}

fn evaluate_body(
    module: &Module,
    function: &Function,
    body: crate::contract::ir::SeqId,
    arguments: Vec<Const>,
    active: &mut Vec<FnId>,
    fuel: &mut usize,
) -> Result<Outcome, &'static str> {
    let mut values = function.params.iter().copied().zip(arguments).collect();
    match evaluate_sequence(module, function, body, &mut values, active, fuel, 0)? {
        Control::Return(outcome) => Ok(outcome),
        Control::Yield(_) => Err("const function body yields instead of returning"),
    }
}

enum Control {
    Yield(Const),
    Return(Outcome),
}
fn evaluate_sequence(
    module: &Module,
    function: &Function,
    body: crate::contract::ir::SeqId,
    values: &mut HashMap<VarId, Const>,
    active: &mut Vec<FnId>,
    fuel: &mut usize,
    depth: usize,
) -> Result<Control, &'static str> {
    if *fuel == 0 || depth >= 128 {
        return Err("static const control-flow evaluation budget exhausted");
    }
    *fuel -= 1;
    let sequence = &function.seqs[body];
    for &statement_id in &sequence.stmts {
        if *fuel == 0 {
            return Err("static const evaluation budget exhausted");
        }
        *fuel -= 1;
        let (dst, outcome) = match &function.stmts[statement_id] {
            Stmt::Let {
                dst,
                value,
                on_defect: None,
                on_suspend: None,
            } => (
                *dst,
                evaluate_value(module, function, value, values, active, fuel)?,
            ),
            Stmt::If {
                dst,
                cond,
                then,
                else_,
            } => {
                let Const::Bool(condition) = operand_value(cond, values)? else {
                    return Err("const If condition is not Bool");
                };
                let branch = if condition { *then } else { *else_ };
                match evaluate_sequence(module, function, branch, values, active, fuel, depth + 1)?
                {
                    Control::Yield(value) => (*dst, Outcome::Value(value)),
                    control => return Ok(control),
                }
            }
            Stmt::Switch {
                dst,
                on,
                cases,
                default,
            } => {
                let key = match on {
                    SwitchOn::Tag(base) => {
                        CaseKey::Variant(unit_enum_tag(module, function, *base, values)?)
                    }
                    SwitchOn::Lit(base) => {
                        CaseKey::Lit(operand_value(&Operand::Var(*base), values)?)
                    }
                };
                let branch = cases
                    .iter()
                    .find(|case| case.key == key)
                    .map(|case| case.body)
                    .or(*default)
                    .ok_or("const Switch has no matching case")?;
                match evaluate_sequence(module, function, branch, values, active, fuel, depth + 1)?
                {
                    Control::Yield(value) => (*dst, Outcome::Value(value)),
                    control => return Ok(control),
                }
            }
            _ => return Err("const body contains unsupported control flow"),
        };
        match outcome {
            Outcome::Value(value) => {
                if let Some(dst) = dst {
                    values.insert(dst, value);
                }
            }
            outcome => return Ok(Control::Return(outcome)),
        }
    }
    Ok(match &sequence.term {
        Term::Return(value) => Control::Return(Outcome::Value(
            value
                .as_ref()
                .map(|value| operand_value(value, values))
                .transpose()?
                .unwrap_or(Const::Unit),
        )),
        Term::Yield(value) => Control::Yield(
            value
                .as_ref()
                .map(|value| operand_value(value, values))
                .transpose()?
                .unwrap_or(Const::Unit),
        ),
        Term::Defect(operand) => {
            let Const::String(message) = operand_value(operand, values)? else {
                return Err("const defect message is not a String");
            };
            Control::Return(Outcome::Defect {
                kind: defect_kind(&message),
                message,
            })
        }
        _ => return Err("const body has an unsupported terminator"),
    })
}

fn unit_enum(module: &Module, id: EnumId) -> bool {
    // The existing backend ABI represents only this closed, payload-free shape
    // as an i32. Other enum values keep their normal runtime const path.
    let enumeration = &module.core.enums[id];
    !enumeration.error
        && !enumeration.external
        && !enumeration.variants.is_empty()
        && enumeration
            .variants
            .iter()
            .all(|variant| variant.tys.is_empty())
}
fn unit_enum_tag(
    module: &Module,
    function: &Function,
    base: VarId,
    values: &HashMap<VarId, Const>,
) -> Result<usize, &'static str> {
    let Type::Enum(id, _) = module.core.ty(function.vars[base].ty) else {
        return Err("const Tag operand is not an enum");
    };
    if !unit_enum(module, *id) {
        return Err("const Tag enum is not represented by a scalar tag");
    }
    let Const::Int(tag) = operand_value(&Operand::Var(base), values)? else {
        return Err("const Tag operand has no scalar tag");
    };
    usize::try_from(tag)
        .ok()
        .filter(|tag| *tag < module.core.enums[*id].variants.len())
        .ok_or("const Tag value is not an enum variant")
}

fn evaluate_value(
    module: &Module,
    function: &Function,
    value: &Value,
    values: &HashMap<VarId, Const>,
    active: &mut Vec<FnId>,
    fuel: &mut usize,
) -> Result<Outcome, &'static str> {
    match value {
        Value::Make { ctor, args } if args.is_empty() && unit_enum(module, ctor.enum_id) => {
            if ctor.variant >= module.core.enums[ctor.enum_id].variants.len() {
                return Err("const constructor variant is invalid");
            }
            let tag =
                i32::try_from(ctor.variant).map_err(|_| "const enum tag exceeds Int range")?;
            Ok(Outcome::Value(Const::Int(tag)))
        }
        Value::Tag(base) => {
            let tag = unit_enum_tag(module, function, *base, values)?;
            Ok(Outcome::Value(Const::Int(
                i32::try_from(tag).map_err(|_| "const enum tag exceeds Int range")?,
            )))
        }
        Value::Const(value) => Ok(Outcome::Value(value.clone())),
        Value::Prim { op, ty, args } => {
            let args = args
                .iter()
                .map(|arg| operand_value(arg, values))
                .collect::<Result<Vec<_>, _>>()?;
            match evaluate_primitive(*op, *ty, args) {
                Ok(value) => Ok(Outcome::Value(value)),
                Err(message) if message == "division by zero" => Ok(Outcome::Defect {
                    kind: defect_kind(&message),
                    message,
                }),
                Err(_) => Err("const body contains an unsupported primitive"),
            }
        }
        Value::Call {
            callee: Callee::Direct(id),
            args,
            suspends: false,
            on_fail: None,
        } => {
            let arguments = args
                .iter()
                .map(|arg| operand_value(arg, values))
                .collect::<Result<Vec<_>, _>>()?;
            evaluate_function(module, *id, arguments, active, fuel)
        }
        Value::Call { .. } => Err("const body contains an indirect, fallible, or suspending call"),
        Value::Builtin { op, args } => {
            let args = args
                .iter()
                .map(|arg| operand_value(arg, values))
                .collect::<Result<Vec<_>, _>>()?;
            match evaluate_builtin(*op, args) {
                Ok(value) => Ok(Outcome::Value(value)),
                Err("float conversion to Int is out of range") => Ok(Outcome::Defect {
                    kind: "FLOAT_TO_INT",
                    message: "cannot convert Float to Int".to_owned(),
                }),
                Err(reason) => Err(reason),
            }
        }
        _ => Err("const body contains a value that is not statically evaluable"),
    }
}

fn evaluate_builtin(op: Builtin, args: Vec<Const>) -> Result<Const, &'static str> {
    match (op, args.as_slice()) {
        (Builtin::StringAppend, [Const::String(left), Const::String(right)]) => {
            // A few repeated doublings must not allocate exponentially in the compiler.
            if left
                .len()
                .checked_add(right.len())
                .is_none_or(|bytes| bytes > usize::from(u16::MAX))
            {
                return Err("static String evaluation byte budget exhausted");
            }
            Ok(Const::String(format!("{left}{right}")))
        }
        (Builtin::IntToString, [Const::Int(value)]) => Ok(Const::String(value.to_string())),
        (Builtin::Int64ToString, [Const::Int64(value)]) => Ok(Const::String(value.to_string())),
        (Builtin::FloatToString, [Const::Float(value)]) => Ok(Const::String(format_float(*value))),
        (Builtin::FloatFromInt, [Const::Int(value)]) => Ok(Const::Float(f64::from(*value))),
        (Builtin::FloatFloor, [Const::Float(value)]) => float_to_int(value.floor()),
        (Builtin::FloatCeil, [Const::Float(value)]) => float_to_int(value.ceil()),
        (Builtin::FloatRound, [Const::Float(value)]) => float_to_int(value.round()),
        (Builtin::FloatTruncate, [Const::Float(value)]) => float_to_int(value.trunc()),
        (Builtin::StringByteSize, [Const::String(value)]) => i32::try_from(value.len())
            .map(Const::Int)
            .map_err(|_| "String byte size exceeds Int range"),
        (Builtin::StringLength, [Const::String(value)]) => i32::try_from(grapheme_count(value))
            .map(Const::Int)
            .map_err(|_| "String grapheme count exceeds Int range"),
        (Builtin::StringContains, [Const::String(value), Const::String(needle)]) => {
            Ok(Const::Bool(value.contains(needle.as_str())))
        }
        (Builtin::StringStartsWith, [Const::String(value), Const::String(prefix)]) => {
            Ok(Const::Bool(value.starts_with(prefix.as_str())))
        }
        (Builtin::StringEndsWith, [Const::String(value), Const::String(suffix)]) => {
            Ok(Const::Bool(value.ends_with(suffix.as_str())))
        }
        (Builtin::StringTrim, [Const::String(value)]) => Ok(Const::String(
            value
                .trim_matches(|character: char| is_white_space(character as u32))
                .to_owned(),
        )),
        _ => Err("const body contains an unsupported builtin or builtin operand"),
    }
}

use crate::semantics::numeric::format_float;

fn float_to_int(value: f64) -> Result<Const, &'static str> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err("float conversion to Int is out of range");
    }
    Ok(Const::Int(value as i32))
}

fn is_white_space(code_point: u32) -> bool {
    (0x0009..=0x000d).contains(&code_point)
        || matches!(
            code_point,
            0x0020 | 0x0085 | 0x00a0 | 0x1680 | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000
        )
        || (0x2000..=0x200a).contains(&code_point)
}

fn operand_value(operand: &Operand, values: &HashMap<VarId, Const>) -> Result<Const, &'static str> {
    match operand {
        Operand::Const(value) => Ok(value.clone()),
        Operand::Var(variable) => values
            .get(variable)
            .cloned()
            .ok_or("const body reads a value that is not statically available"),
    }
}

fn evaluate_primitive(op: PrimOp, ty: PrimTy, args: Vec<Const>) -> Result<Const, String> {
    if args.len()
        != (if matches!(op, PrimOp::Neg | PrimOp::Not) {
            1
        } else {
            2
        })
    {
        return Err("invalid primitive arity".to_owned());
    }
    let defect = || "division by zero".to_owned();

    match ty {
        PrimTy::I32 => {
            let lhs = args.first().and_then(as_i32).ok_or("invalid Int operand")?;
            if op == PrimOp::Not {
                if args.len() != 1 || !matches!(args[0], Const::Bool(_)) {
                    return Err("invalid Bool operand".to_owned());
                }
                return Ok(Const::Bool(lhs == 0));
            }
            if op == PrimOp::Neg {
                return Ok(Const::Int(lhs.wrapping_neg()));
            }
            let rhs = args.get(1).and_then(as_i32).ok_or("invalid Int operand")?;
            match op {
                PrimOp::Add => Ok(Const::Int(lhs.wrapping_add(rhs))),
                PrimOp::Sub => Ok(Const::Int(lhs.wrapping_sub(rhs))),
                PrimOp::Mul => Ok(Const::Int(lhs.wrapping_mul(rhs))),
                PrimOp::Div if rhs == 0 => Err(defect()),
                PrimOp::Div if lhs == i32::MIN && rhs == -1 => Ok(Const::Int(i32::MIN)),
                PrimOp::Div => Ok(Const::Int(lhs / rhs)),
                PrimOp::Rem if rhs == 0 => Err(defect()),
                PrimOp::Rem if lhs == i32::MIN && rhs == -1 => Ok(Const::Int(0)),
                PrimOp::Rem => Ok(Const::Int(lhs % rhs)),
                PrimOp::Eq => Ok(Const::Bool(lhs == rhs)),
                PrimOp::Ne => Ok(Const::Bool(lhs != rhs)),
                PrimOp::Lt => Ok(Const::Bool(lhs < rhs)),
                PrimOp::Le => Ok(Const::Bool(lhs <= rhs)),
                PrimOp::Gt => Ok(Const::Bool(lhs > rhs)),
                PrimOp::Ge => Ok(Const::Bool(lhs >= rhs)),
                PrimOp::Neg | PrimOp::Not => unreachable!(),
            }
        }
        PrimTy::I64 => {
            let lhs = args
                .first()
                .and_then(as_i64)
                .ok_or("invalid Int64 operand")?;
            if op == PrimOp::Neg {
                return Ok(Const::Int64(lhs.wrapping_neg()));
            }
            let rhs = args
                .get(1)
                .and_then(as_i64)
                .ok_or("invalid Int64 operand")?;
            match op {
                PrimOp::Add => Ok(Const::Int64(lhs.wrapping_add(rhs))),
                PrimOp::Sub => Ok(Const::Int64(lhs.wrapping_sub(rhs))),
                PrimOp::Mul => Ok(Const::Int64(lhs.wrapping_mul(rhs))),
                PrimOp::Div if rhs == 0 => Err(defect()),
                PrimOp::Div if lhs == i64::MIN && rhs == -1 => Ok(Const::Int64(i64::MIN)),
                PrimOp::Div => Ok(Const::Int64(lhs / rhs)),
                PrimOp::Rem if rhs == 0 => Err(defect()),
                PrimOp::Rem if lhs == i64::MIN && rhs == -1 => Ok(Const::Int64(0)),
                PrimOp::Rem => Ok(Const::Int64(lhs % rhs)),
                PrimOp::Eq => Ok(Const::Bool(lhs == rhs)),
                PrimOp::Ne => Ok(Const::Bool(lhs != rhs)),
                PrimOp::Lt => Ok(Const::Bool(lhs < rhs)),
                PrimOp::Le => Ok(Const::Bool(lhs <= rhs)),
                PrimOp::Gt => Ok(Const::Bool(lhs > rhs)),
                PrimOp::Ge => Ok(Const::Bool(lhs >= rhs)),
                PrimOp::Neg | PrimOp::Not => unreachable!(),
            }
        }
        PrimTy::F64 => {
            let lhs = args
                .first()
                .and_then(as_f64)
                .ok_or("invalid Float operand")?;
            if op == PrimOp::Neg {
                return Ok(Const::Float(-lhs));
            }
            let rhs = args
                .get(1)
                .and_then(as_f64)
                .ok_or("invalid Float operand")?;
            match op {
                PrimOp::Add => Ok(Const::Float(lhs + rhs)),
                PrimOp::Sub => Ok(Const::Float(lhs - rhs)),
                PrimOp::Mul => Ok(Const::Float(lhs * rhs)),
                PrimOp::Div => Ok(Const::Float(lhs / rhs)),
                PrimOp::Rem => Ok(Const::Float(lhs % rhs)),
                PrimOp::Eq => Ok(Const::Bool(lhs == rhs)),
                PrimOp::Ne => Ok(Const::Bool(lhs != rhs)),
                PrimOp::Lt => Ok(Const::Bool(lhs < rhs)),
                PrimOp::Le => Ok(Const::Bool(lhs <= rhs)),
                PrimOp::Gt => Ok(Const::Bool(lhs > rhs)),
                PrimOp::Ge => Ok(Const::Bool(lhs >= rhs)),
                PrimOp::Neg | PrimOp::Not => unreachable!(),
            }
        }
        PrimTy::String => {
            let [Const::String(lhs), Const::String(rhs)] = args.as_slice() else {
                return Err("invalid String operand".to_owned());
            };
            match op {
                PrimOp::Eq => Ok(Const::Bool(lhs == rhs)),
                PrimOp::Ne => Ok(Const::Bool(lhs != rhs)),
                PrimOp::Lt => Ok(Const::Bool(lhs < rhs)),
                PrimOp::Le => Ok(Const::Bool(lhs <= rhs)),
                PrimOp::Gt => Ok(Const::Bool(lhs > rhs)),
                PrimOp::Ge => Ok(Const::Bool(lhs >= rhs)),
                _ => Err("unsupported String primitive".to_owned()),
            }
        }
    }
}

fn as_i32(value: &Const) -> Option<i32> {
    match value {
        Const::Int(value) => Some(*value),
        Const::Bool(value) => Some(i32::from(*value)),
        _ => None,
    }
}

fn as_i64(value: &Const) -> Option<i64> {
    match value {
        Const::Int64(value) => Some(*value),
        _ => None,
    }
}

fn as_f64(value: &Const) -> Option<f64> {
    match value {
        Const::Float(value) => Some(*value),
        _ => None,
    }
}

fn defect_kind(message: &str) -> &'static str {
    if message == "not yet implemented" {
        "TODO"
    } else if message.starts_with("unreachable: no match arm matched") {
        "NO_MATCH"
    } else if message.starts_with("assertion failed") {
        "ASSERT"
    } else if message == "division by zero" {
        "DIVISION_BY_ZERO"
    } else {
        "PANIC"
    }
}

#[cfg(test)]
mod tests {
    use crate::contract::ir::Builtin;
    use crate::contract::ir::Const;

    use super::{evaluate_builtin, format_float};

    #[test]
    fn formats_float_edges_with_emela_decimal_notation() {
        for (value, expected) in [
            (0.0, "0.0"),
            (-0.0, "0.0"),
            (0.1, "0.1"),
            (1.0e-6, "0.000001"),
            (1.0e-7, "1e-7"),
            (1.0e20, "100000000000000000000.0"),
            (1.0e21, "1e+21"),
            (f64::from_bits(1), "5e-324"),
            (f64::MIN_POSITIVE, "2.2250738585072014e-308"),
            (f64::MAX, "1.7976931348623157e+308"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(format_float(value), expected);
        }
        assert_eq!(format_float(f64::NAN), "NaN");
    }

    #[test]
    fn evaluates_unicode_string_length_with_the_pinned_grapheme_data() {
        assert_eq!(
            evaluate_builtin(
                Builtin::StringLength,
                vec![Const::String("a\u{0308}\r\n👩‍👩‍👧‍👦".to_owned())],
            ),
            Ok(Const::Int(3))
        );
    }
}
