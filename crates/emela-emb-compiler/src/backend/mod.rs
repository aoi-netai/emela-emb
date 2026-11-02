//! Pure orchestration over the independent backend contract.
pub(crate) mod emit;
pub(crate) mod lower;
pub(crate) mod plan;

pub(crate) fn compile_program(
    program: crate::contract::ir::Program,
    options: &crate::Options,
) -> Result<crate::Output, Vec<crate::Error>> {
    let lowered = lower::lower_program(program)?;
    let planned = plan::prepare(&lowered, options)?;
    emit::render(&planned)
}
