use crate::contract::{CType, LoweredModule, PlannedModule};

pub(crate) fn plan(module: LoweredModule) -> PlannedModule {
    PlannedModule {
        symbol: module.export_name,
        return_type: CType::Int32,
        return_value: module.return_value,
    }
}
