use crate::contract::LoweredModule;
use crate::frontend::check::CheckedModule;

pub(crate) fn lower(module: CheckedModule<'_>) -> LoweredModule {
    LoweredModule {
        export_name: module.export_symbol.to_owned(),
        return_value: module.value,
    }
}
