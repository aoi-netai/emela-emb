// Keep analysis dispatch exhaustive: every new variant needs an explicit planning decision.
use super::*;
pub(in crate::backend::emit) mod as_error;
pub(in crate::backend::emit) mod builtin;
pub(in crate::backend::emit) mod call;
pub(in crate::backend::emit) mod closure;
pub(in crate::backend::emit) mod coerce;
pub(in crate::backend::emit) mod const_;
pub(in crate::backend::emit) mod env_get;
pub(in crate::backend::emit) mod ev_extend;
pub(in crate::backend::emit) mod ev_lookup;
pub(in crate::backend::emit) mod fiber;
pub(in crate::backend::emit) mod field;
pub(in crate::backend::emit) mod fn_ref;
pub(in crate::backend::emit) mod handler;
pub(in crate::backend::emit) mod is_error;
pub(in crate::backend::emit) mod list;
pub(in crate::backend::emit) mod list_head;
pub(in crate::backend::emit) mod list_is_empty;
pub(in crate::backend::emit) mod list_tail;
pub(in crate::backend::emit) mod make;
pub(in crate::backend::emit) mod make_closure;
pub(in crate::backend::emit) mod prim;
pub(in crate::backend::emit) mod runtime;
pub(in crate::backend::emit) mod tag;
pub(in crate::backend::emit) mod tuple;
pub(in crate::backend::emit) mod tuple_get;
pub(in crate::backend::emit) fn emit(
    context: &mut Context<'_, '_>,
    dst: &Option<VarId>,
    value: &Value,
) -> Result<(), Vec<Error>> {
    match value {
        Value::Const(_) => const_::emit(context, dst, value),
        Value::Prim { .. } => prim::emit(context, dst, value),
        Value::Call { .. } => call::emit(context, dst, value),
        Value::Builtin { .. } => builtin::emit(context, dst, value),
        Value::Make { .. } => make::emit(context, dst, value),
        Value::Field { .. } => field::emit(context, dst, value),
        Value::Tag(_) => tag::emit(context, dst, value),
        Value::Tuple(_) => tuple::emit(context, dst, value),
        Value::TupleGet { .. } => tuple_get::emit(context, dst, value),
        Value::List { .. } => list::emit(context, dst, value),
        Value::ListIsEmpty(_) => list_is_empty::emit(context, dst, value),
        Value::ListHead(_) => list_head::emit(context, dst, value),
        Value::ListTail(_) => list_tail::emit(context, dst, value),
        Value::FnRef(_) => fn_ref::emit(context, dst, value),
        Value::Closure(_) => closure::emit(context, dst, value),
        Value::MakeClosure { .. } => make_closure::emit(context, dst, value),
        Value::EnvGet { .. } => env_get::emit(context, dst, value),
        Value::EvLookup { .. } => ev_lookup::emit(context, dst, value),
        Value::EvExtend { .. } => ev_extend::emit(context, dst, value),
        Value::Handler { .. } => handler::emit(context, dst, value),
        Value::Fiber { .. } => fiber::emit(context, dst, value),
        Value::IsError { .. } => is_error::emit(context, dst, value),
        Value::AsError { .. } => as_error::emit(context, dst, value),
        Value::Runtime { .. } => runtime::emit(context, dst, value),
        Value::Coerce(_) => coerce::emit(context, dst, value),
    }
}
