use super::*;
pub(in crate::backend::emit) mod builtin_utilities;
pub(in crate::backend::emit) mod builtins;
pub(in crate::backend::emit) mod statements;
pub(in crate::backend::emit) mod values;
pub(in crate::backend::emit) struct Context<'a, 'ir> {
    pub(in crate::backend::emit) layouts: &'a TypeLayouts<'ir>,
    pub(in crate::backend::emit) function: &'a Function,
    pub(in crate::backend::emit) writer: &'a mut Writer,
    pub(in crate::backend::emit) names: &'a HashMap<FnId, String>,
    pub(in crate::backend::emit) string_buffers: &'a StringBufferPlan,
    pub(in crate::backend::emit) loops: &'a mut Vec<Vec<VarId>>,
    pub(in crate::backend::emit) blocks: &'a mut Vec<BlockTarget>,
    pub(in crate::backend::emit) protects: &'a mut Vec<ProtectTarget>,
    pub(in crate::backend::emit) next_temp: &'a mut usize,
    pub(in crate::backend::emit) next_label: &'a mut usize,
}
pub(in crate::backend::emit) struct BuiltinContext<'a, 'ir> {
    pub(in crate::backend::emit) layouts: &'a TypeLayouts<'ir>,
    pub(in crate::backend::emit) function: &'a Function,
    pub(in crate::backend::emit) string_buffers: &'a StringBufferPlan,
    pub(in crate::backend::emit) writer: &'a mut Writer,
}
use builtin_utilities::ResultType;

type Emission = Result<(), Vec<Error>>;

pub(in crate::backend::emit) mod constants;

pub(in crate::backend::emit) mod operands;

pub(in crate::backend::emit) mod primitive_operations;

pub(in crate::backend::emit) mod primitive_types;

pub(in crate::backend::emit) mod terms;

pub(in crate::backend::emit) mod failures;

pub(in crate::backend::emit) mod runtime;

pub(in crate::backend::emit) mod case_keys;
pub(in crate::backend::emit) mod fibers;
pub(in crate::backend::emit) mod switches;
use crate::contract::control::ControlNode as Node;
pub(in crate::backend::emit) struct SuspendContext<'a, 'ir> {
    pub(in crate::backend::emit) flow: &'a crate::contract::ir::Program,
    pub(in crate::backend::emit) layouts: &'a TypeLayouts<'ir>,
    pub(in crate::backend::emit) function: &'a Function,
    pub(in crate::backend::emit) writer: &'a mut Writer,
    pub(in crate::backend::emit) names: &'a HashMap<FnId, String>,
    pub(in crate::backend::emit) strings: &'a StringBufferPlan,
    pub(in crate::backend::emit) next_temp: &'a mut usize,
    pub(in crate::backend::emit) next: usize,
    pub(in crate::backend::emit) index: usize,
    pub(in crate::backend::emit) failing: bool,
    pub(in crate::backend::emit) call_error_var: VarId,
    pub(in crate::backend::emit) call_error_label: LabelId,
}

use super::suspend::{emit_operation, emit_sync_statement, prepare_result};

pub(in crate::backend::emit) mod control_utilities;

use super::suspend::retain_frame_value;
