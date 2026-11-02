//! Value types and invocation sets are separate, so later phases can infer sets independently.
use std::fmt;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValueType {
    Int,
    Int64,
    Float,
    Bool,
    String,
    Unit,
    Never,
    Any,
    Compound(crate::contract::ir::TypeId),
}
impl ValueType {
    pub fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Int64 | Self::Float)
    }
}
impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Compound(id) = self {
            return write!(f, "type#{}", id.index());
        }
        f.write_str(match self {
            Self::Int => "Int",
            Self::Int64 => "Int64",
            Self::Float => "Float",
            Self::Bool => "Bool",
            Self::String => "String",
            Self::Unit => "()",
            Self::Never => "Never",
            Self::Any => "EmelaError",
            Self::Compound(_) => unreachable!(),
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InvocationSets {
    pub errors: Vec<crate::contract::ir::TypeId>,
    pub error_constraints: Vec<crate::contract::ir::TypeId>,
    pub effects: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Signature {
    pub params: Vec<ValueType>,
    pub ret: Option<ValueType>,
    pub sets: InvocationSets,
}
