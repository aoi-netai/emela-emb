//! Explicitly shared, pure operations over backend data. No compiler stages or I/O.
// Later parity phases construct the remaining shared contract variants.
#![allow(dead_code)]
pub(crate) mod abi;
pub(crate) mod c;
pub(crate) mod derived;
pub(crate) mod host_modules;
pub(crate) mod host_refs;
pub(crate) mod numeric;
pub(crate) mod representations;
pub(crate) mod strings;
pub(crate) mod type_representations;
mod types;
pub(crate) use types::effect_base;
pub(crate) mod variants;

pub(crate) mod diagnostics;

pub(crate) mod child;
pub(crate) mod deferred;
pub(crate) mod operands;

pub(crate) mod unicode_grapheme;
