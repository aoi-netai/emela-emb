use super::super::*;

pub(in crate::backend::plan) fn result_ownership() -> ResultOwnership {
    ResultOwnership::Borrowed
}

pub(in crate::backend::plan) fn needs_heap() -> bool {
    true
}
