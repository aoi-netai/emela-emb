use super::*;

pub(in crate::backend::plan) mod builtins;
pub(in crate::backend::plan) mod failures;
pub(in crate::backend::plan) mod fibers;
pub(in crate::backend::plan) mod primitive_operations;
pub(in crate::backend::plan) mod runtime;
pub(in crate::backend::plan) mod statements;
pub(in crate::backend::plan) mod terms;
pub(in crate::backend::plan) mod values;

use super::suspend::{Graph, LoopTarget, Node};
pub(in crate::backend::plan) struct LoweringContext<'a> {
    pub(in crate::backend::plan) graph: &'a mut Graph,
    pub(in crate::backend::plan) function: &'a Function,
}
