//! Value types and invocation sets are separate, so later phases can infer sets independently.
use std::fmt;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValueType {
    Int,
    Int64,
    Float,
    Bool,
    Unit,
    Never,
}
impl ValueType {
    pub fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Int64 | Self::Float)
    }
    pub fn compatible(self, expected: Self) -> bool {
        self == expected || self == Self::Never
    }
}
impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Int => "Int",
            Self::Int64 => "Int64",
            Self::Float => "Float",
            Self::Bool => "Bool",
            Self::Unit => "()",
            Self::Never => "Never",
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InvocationSets {
    pub errors: Vec<String>,
    pub effects: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Signature {
    pub params: Vec<ValueType>,
    pub ret: Option<ValueType>,
    pub sets: InvocationSets,
}
