//! Give each source constant a single callable initializer before name resolution.
//!
//! The checker assigns the initializer its definition ID and preserves that origin through
//! specialization. References become calls, so static evaluation and the runtime cache use
//! the same declaration identity regardless of how many modules import the constant.
use super::{ast, modules::Modules};
use crate::Diagnostic;

pub(crate) fn expand(modules: &mut Modules) -> Result<(), Vec<Diagnostic>> {
    for module in &mut modules.modules {
        for declaration in std::mem::take(&mut module.ast.consts) {
            let span = declaration.value.span;
            module.ast.functions.push(ast::Function {
                name: declaration.name,
                public: declaration.public,
                constant: true,
                type_params: Vec::new(),
                params: Vec::new(),
                return_type: declaration.annotation,
                fails: None,
                effects: Some(Vec::new()),
                suspends: false,
                attributes: Vec::new(),
                body: Some(ast::Block {
                    statements: vec![ast::Statement::Expr(declaration.value)],
                    span,
                }),
            });
        }
    }
    Ok(())
}
