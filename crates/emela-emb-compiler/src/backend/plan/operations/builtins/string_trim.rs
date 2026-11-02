use super::super::*;

pub(in crate::backend::plan) fn string_bound(
    args: &[Operand],
    bounds: &StringBounds<'_>,
) -> Option<usize> {
    match args {
        [arg] => Some(bounds.operand(arg)),
        _ => None,
    }
}

pub(in crate::backend::plan) fn result_ownership() -> ResultOwnership {
    ResultOwnership::Borrowed
}

pub(in crate::backend::plan) fn require_runtime(
    _args: &[Operand],
    requirements: &mut crate::contract::runtime::RuntimeRequirements,
) {
    requirements.require(crate::contract::runtime::RuntimeHelper::StringTrim);
}
