use crate::contract::{Output, PlannedModule};

pub(crate) fn emit(module: PlannedModule) -> Output {
    let signature = format!("{} {}(void)", module.return_type.spelling(), module.symbol);
    Output {
        header: format!(
            "#ifndef EMELA_EMBEDDED_H\n#define EMELA_EMBEDDED_H\n\n#include <stdint.h>\n\n{signature};\n\n#endif\n"
        ),
        c_source: format!(
            "#include \"emela_embedded.h\"\n\n{signature} {{\n    return {};\n}}\n",
            module.return_value
        ),
    }
}
