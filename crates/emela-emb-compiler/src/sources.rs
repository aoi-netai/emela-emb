/// A caller-owned UTF-8 source. Names identify files and determine module paths; compilation
/// performs no filesystem access. Give all names relative to the same source root, or use
/// absolute names sharing that root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source<'a> {
    pub name: &'a str,
    pub text: &'a str,
}
impl<'a> Source<'a> {
    pub const fn new(name: &'a str, text: &'a str) -> Self {
        Self { name, text }
    }
}

pub(crate) const CORE_MODULES: &[(&str, &str)] = &[
    ("Int", include_str!("../lib/core/int.emel")),
    ("Int64", include_str!("../lib/core/int64.emel")),
    ("Float", include_str!("../lib/core/float.emel")),
    ("String", include_str!("../lib/core/string.emel")),
    ("Bool", include_str!("../lib/core/bool.emel")),
    ("Unit", include_str!("../lib/core/unit.emel")),
];

/// This identity is assigned only while collecting a declaration from the compiler-owned
/// source modules above. User function spellings never select an intrinsic implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intrinsic {
    IntBitAnd,
    IntBitOr,
    IntBitXor,
    IntBitNot,
    IntShiftLeft,
    IntShiftRight,
    IntShiftRightUnsigned,
    Int64FromInt,
    Int64ToInt,
    Int64BitAnd,
    Int64BitOr,
    Int64BitXor,
    Int64BitNot,
    Int64ShiftLeft,
    Int64ShiftRight,
    Int64ShiftRightUnsigned,
    FloatFromInt,
    IntToString,
    Int64ToString,
    FloatToString,
    FloatFloor,
    FloatCeil,
    FloatRound,
    FloatTruncate,
    StringLength,
    StringByteSize,
    StringAppend,
    StringContains,
    StringStartsWith,
    StringEndsWith,
    StringTrim,
}
pub(crate) fn intrinsic(module: &str, name: &str) -> Option<Intrinsic> {
    Some(match (module, name) {
        ("Int", "bit_and") => Intrinsic::IntBitAnd,
        ("Int", "bit_or") => Intrinsic::IntBitOr,
        ("Int", "bit_xor") => Intrinsic::IntBitXor,
        ("Int", "bit_not") => Intrinsic::IntBitNot,
        ("Int", "shift_left") => Intrinsic::IntShiftLeft,
        ("Int", "shift_right") => Intrinsic::IntShiftRight,
        ("Int", "shift_right_unsigned") => Intrinsic::IntShiftRightUnsigned,
        ("Int64", "from_int") => Intrinsic::Int64FromInt,
        ("Int64", "to_int") => Intrinsic::Int64ToInt,
        ("Int64", "bit_and") => Intrinsic::Int64BitAnd,
        ("Int64", "bit_or") => Intrinsic::Int64BitOr,
        ("Int64", "bit_xor") => Intrinsic::Int64BitXor,
        ("Int64", "bit_not") => Intrinsic::Int64BitNot,
        ("Int64", "shift_left") => Intrinsic::Int64ShiftLeft,
        ("Int64", "shift_right") => Intrinsic::Int64ShiftRight,
        ("Int64", "shift_right_unsigned") => Intrinsic::Int64ShiftRightUnsigned,
        ("Float", "from_int") => Intrinsic::FloatFromInt,
        ("Int", "to_string") => Intrinsic::IntToString,
        ("Int64", "to_string") => Intrinsic::Int64ToString,
        ("Float", "to_string") => Intrinsic::FloatToString,
        ("Float", "floor") => Intrinsic::FloatFloor,
        ("Float", "ceil") => Intrinsic::FloatCeil,
        ("Float", "round") => Intrinsic::FloatRound,
        ("Float", "truncate") => Intrinsic::FloatTruncate,
        ("String", "length") => Intrinsic::StringLength,
        ("String", "byte_size") => Intrinsic::StringByteSize,
        ("String", "append") => Intrinsic::StringAppend,
        ("String", "contains") => Intrinsic::StringContains,
        ("String", "starts_with") => Intrinsic::StringStartsWith,
        ("String", "ends_with") => Intrinsic::StringEndsWith,
        ("String", "trim") => Intrinsic::StringTrim,
        _ => return None,
    })
}
