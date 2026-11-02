/// The complete generated files. The caller owns any persistence or I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub c_source: String,
    pub header: String,
}

/// The backend's independent representation of the supported exported function.
pub(crate) struct LoweredModule {
    pub export_name: String,
    pub return_value: i32,
}

pub(crate) enum CType {
    Int32,
}

impl CType {
    pub fn spelling(&self) -> &'static str {
        match self {
            Self::Int32 => "int32_t",
        }
    }
}

pub(crate) struct PlannedModule {
    pub symbol: String,
    pub return_type: CType,
    pub return_value: i32,
}
