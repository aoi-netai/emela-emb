//! Give frame planning an owned function with a private call-failure landing slot.
use super::*;
use crate::contract::lowered::SuspensionInput;

pub(super) fn prepare(program: &Program) -> HashMap<FnId, SuspensionInput> {
    program
        .functions
        .iter()
        .filter_map(|(id, function)| {
            if !function.suspends || function.external.is_some() {
                return None;
            }
            let mut function = function.clone();
            let error_variable = function.vars.alloc(VarData {
                hint: "call_error".into(),
                ty: TypeId::ANY,
                repr: Repr::Ref,
            });
            let error_label = function.labels.alloc(LabelData {
                hint: "call_error".into(),
            });
            Some((
                id,
                SuspensionInput {
                    function,
                    error_variable,
                    error_label,
                },
            ))
        })
        .collect()
}
