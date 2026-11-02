//! Source effect identities and concrete instances, followed by handler/layer collection.
use super::{ast, modules::Modules};
use crate::{Diagnostic, DiagnosticKind as DK, Span, contract::ir as b};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct EffectDeclaration {
    pub module: usize,
    pub ast: ast::Effect,
    /// Template identity, reserved before resolving any field or operation signature.
    pub id: b::EffectId,
}

#[derive(Default)]
pub(crate) struct EffectRegistry {
    pub declarations: Vec<EffectDeclaration>,
    names: Vec<HashMap<String, usize>>,
    instances: HashMap<(usize, Vec<b::TypeId>), b::EffectId>,
    instance_declarations: HashMap<b::EffectId, usize>,
}

impl EffectRegistry {
    pub fn declare(modules: &Modules, core: &mut b::Definitions) -> Result<Self, Vec<Diagnostic>> {
        let mut registry = Self {
            names: vec![HashMap::new(); modules.modules.len()],
            ..Self::default()
        };
        let mut errors = Vec::new();
        for (module, source) in modules.modules.iter().enumerate() {
            for effect in &source.ast.effects {
                let result = (|| -> Result<(), Diagnostic> {
                    if registry.names[module].contains_key(&effect.name.value)
                        || source
                            .ast
                            .data
                            .iter()
                            .any(|data| data.name.value == effect.name.value)
                        || source
                            .ast
                            .traits
                            .iter()
                            .any(|item| item.name.value == effect.name.value)
                        || source
                            .ast
                            .handlers
                            .iter()
                            .any(|item| item.name.value == effect.name.value)
                        || source
                            .ast
                            .layers
                            .iter()
                            .any(|item| item.name.value == effect.name.value)
                    {
                        return Err(Diagnostic::new(
                            DK::Name,
                            effect.name.span,
                            "duplicate effect/type declaration",
                        ));
                    }
                    if effect
                        .attributes
                        .iter()
                        .any(|attribute| !matches!(attribute, ast::Attribute::Intrinsic(_)))
                        || (!source.builtin && !effect.attributes.is_empty())
                    {
                        return Err(Diagnostic::new(
                            DK::Type,
                            effect.name.span,
                            "intrinsic effects require a compiler-owned declaration",
                        ));
                    }
                    let mut names = HashSet::new();
                    for parameter in &effect.params {
                        if !names.insert(parameter.name.value.as_str()) {
                            return Err(Diagnostic::new(
                                DK::Name,
                                parameter.name.span,
                                "duplicate effect type parameter",
                            ));
                        }
                    }
                    let mut names = HashSet::new();
                    for operation in &effect.operations {
                        if !names.insert(operation.name.value.as_str()) {
                            return Err(Diagnostic::new(
                                DK::Name,
                                operation.name.span,
                                "duplicate effect operation",
                            ));
                        }
                        let mut parameters = HashSet::new();
                        for parameter in &operation.params {
                            if !parameters.insert(parameter.name.value.as_str()) {
                                return Err(Diagnostic::new(
                                    DK::Name,
                                    parameter.name.span,
                                    "duplicate operation parameter",
                                ));
                            }
                        }
                        if (!source.builtin || effect.attributes.is_empty())
                            && !operation.type_params.is_empty()
                        {
                            return Err(Diagnostic::new(
                                DK::Type,
                                operation.type_params[0].name.span,
                                "only intrinsic effects may declare polymorphic operations",
                            ));
                        }
                    }
                    if effect.underlying.is_some()
                        && (!effect.params.is_empty() || !effect.operations.is_empty())
                    {
                        return Err(Diagnostic::new(
                            DK::Type,
                            effect.name.span,
                            "effect labels cannot have type parameters or operations",
                        ));
                    }
                    let decl = declaration_name(&source.name, &effect.name.value);
                    let args: Vec<_> = (0..effect.params.len())
                        .map(|index| core.types.intern(b::Type::Param(index)))
                        .collect();
                    let id = core.effects.alloc(b::EffectDef {
                        name: decl.clone(),
                        decl,
                        args,
                        ops: Vec::new(),
                        label_of: None,
                    });
                    let index = registry.declarations.len();
                    registry.names[module].insert(effect.name.value.clone(), index);
                    registry.instance_declarations.insert(id, index);
                    if effect.params.is_empty() {
                        registry.instances.insert((index, Vec::new()), id);
                    }
                    registry.declarations.push(EffectDeclaration {
                        module,
                        ast: effect.clone(),
                        id,
                    });
                    Ok(())
                })();
                if let Err(error) = result {
                    errors.push(error.in_source(&source.source_name));
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        for declaration in &registry.declarations {
            if let Some(underlying) = &declaration.ast.underlying {
                let head = underlying
                    .value
                    .split('[')
                    .next()
                    .unwrap_or(&underlying.value);
                let result = registry.find_declaration(
                    modules,
                    declaration.module,
                    &ast::Text::new(head, underlying.span),
                );
                match result {
                    Err(error) => errors.push(error),
                    Ok(index) => {
                        let target = &registry.declarations[index];
                        if target.ast.underlying.is_some()
                            || target.ast.operations.is_empty()
                            || !target.ast.attributes.is_empty()
                        {
                            errors.push(Diagnostic::new(DK::Type, underlying.span, "a label must name a user effect with operations, not a label or intrinsic effect").in_source(&modules.modules[declaration.module].source_name));
                        }
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(registry)
        } else {
            Err(errors)
        }
    }

    fn find_declaration(
        &self,
        modules: &Modules,
        module: usize,
        name: &ast::Text,
    ) -> Result<usize, Diagnostic> {
        let source = modules.modules.get(module);
        let fail = || {
            let error = Diagnostic::new(
                DK::Name,
                name.span,
                format!("unknown, private, or ambiguous effect '{}'", name.value),
            );
            match source {
                Some(source) => error.in_source(&source.source_name),
                None => error,
            }
        };
        let source = source.ok_or_else(fail)?;
        if let Some((prefix, member)) = name.value.rsplit_once('.') {
            let target = source
                .ast
                .imports
                .iter()
                .find(|import| {
                    import.path.value == prefix
                        || import.path.value.rsplit('.').next() == Some(prefix)
                })
                .and_then(|import| {
                    modules
                        .modules
                        .iter()
                        .position(|item| item.name == import.path.value)
                })
                .or_else(|| {
                    (prefix == "Prelude")
                        .then(|| {
                            modules
                                .modules
                                .iter()
                                .position(|item| item.builtin && item.name == "Prelude")
                        })
                        .flatten()
                })
                .ok_or_else(fail)?;
            let &index = self.names[target].get(member).ok_or_else(fail)?;
            return self.declarations[index]
                .ast
                .public
                .then_some(index)
                .ok_or_else(fail);
        }
        if let Some(&index) = self.names[module].get(&name.value) {
            return Ok(index);
        }
        let mut found = None;
        let mut explicit = false;
        for import in &source.ast.imports {
            if !import
                .members
                .as_ref()
                .is_some_and(|members| members.iter().any(|item| item.value == name.value))
            {
                continue;
            }
            explicit = true;
            let target = modules
                .modules
                .iter()
                .position(|item| item.name == import.path.value)
                .ok_or_else(fail)?;
            let &index = self.names[target].get(&name.value).ok_or_else(fail)?;
            if !self.declarations[index].ast.public
                || found.is_some_and(|previous| previous != index)
            {
                return Err(fail());
            }
            found = Some(index);
        }
        if explicit {
            return found.ok_or_else(fail);
        }
        let prelude = modules
            .modules
            .iter()
            .position(|item| item.builtin && item.name == "Prelude")
            .ok_or_else(fail)?;
        self.names[prelude]
            .get(&name.value)
            .copied()
            .filter(|&index| self.declarations[index].ast.public)
            .ok_or_else(fail)
    }

    pub fn resolve_instance(
        &mut self,
        modules: &Modules,
        core: &mut b::Definitions,
        module: usize,
        name: &ast::Text,
        args: Vec<b::TypeId>,
    ) -> Result<b::EffectId, Diagnostic> {
        let index = self.find_declaration(modules, module, name)?;
        let declaration = &self.declarations[index];
        if args.len() != declaration.ast.params.len() {
            return Err(Diagnostic::new(
                DK::Type,
                name.span,
                format!(
                    "effect '{}' expects {} type arguments, found {}",
                    name.value,
                    declaration.ast.params.len(),
                    args.len()
                ),
            )
            .in_source(&modules.modules[module].source_name));
        }
        if args.iter().any(|arg| arg.index() >= core.types.len()) {
            return Err(Diagnostic::new(
                DK::Type,
                name.span,
                "effect instance has an invalid type argument",
            )
            .in_source(&modules.modules[module].source_name));
        }
        if let Some(&id) = self.instances.get(&(index, args.clone())) {
            return Ok(id);
        }
        let template = core.effects[declaration.id].clone();
        let ops = instantiate_operations(core, &template.ops, &args);
        let id = core.effects.alloc(b::EffectDef {
            name: format!(
                "{}[{}]",
                template.decl,
                args.iter()
                    .map(|arg| type_name(core, *arg))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            decl: template.decl,
            args: args.clone(),
            ops,
            label_of: template.label_of,
        });
        self.instances.insert((index, args), id);
        self.instance_declarations.insert(id, index);
        Ok(id)
    }

    /// Signatures can mention capabilities before data fields and operations have been filled.
    /// Update those already-reserved instances once the template signatures are available.
    pub fn refresh_instances(&mut self, core: &mut b::Definitions) {
        let instances: Vec<_> = self
            .instances
            .iter()
            .map(|((index, args), &id)| (*index, args.clone(), id))
            .collect();
        for (index, args, id) in instances {
            let template = core.effects[self.declarations[index].id].clone();
            if id != self.declarations[index].id {
                core.effects[id].ops = instantiate_operations(core, &template.ops, &args);
                core.effects[id].label_of = template.label_of;
            }
        }
    }

    pub fn declaration(&self, id: b::EffectId) -> Option<&EffectDeclaration> {
        self.instance_declarations
            .get(&id)
            .map(|&index| &self.declarations[index])
    }
}

fn declaration_name(module: &str, name: &str) -> String {
    if module == "Prelude" {
        name.to_owned()
    } else {
        format!("{module}.{name}")
    }
}

fn instantiate_operations(
    core: &mut b::Definitions,
    operations: &[b::OpDef],
    args: &[b::TypeId],
) -> Vec<b::OpDef> {
    operations
        .iter()
        .map(|operation| {
            let mut substitution = args.to_vec();
            substitution.extend(
                (0..operation.type_params).map(|index| core.types.intern(b::Type::Param(index))),
            );
            b::OpDef {
                name: operation.name.clone(),
                suspend: operation.suspend,
                type_params: operation.type_params,
                params: operation
                    .params
                    .iter()
                    .map(|&ty| core.types.subst(ty, &substitution))
                    .collect(),
                ret: core.types.subst(operation.ret, &substitution),
                fails: operation.fails,
            }
        })
        .collect()
}

fn type_name(core: &b::Definitions, ty: b::TypeId) -> String {
    match core.ty(ty) {
        b::Type::Int => "Int".into(),
        b::Type::Int64 => "Int64".into(),
        b::Type::Float => "Float".into(),
        b::Type::Bool => "Bool".into(),
        b::Type::String => "String".into(),
        b::Type::Unit => "Unit".into(),
        b::Type::Never => "Never".into(),
        b::Type::Param(index) => format!("T{index}"),
        b::Type::List(element) => format!("List[{}]", type_name(core, *element)),
        b::Type::Enum(id, args) => {
            if args.is_empty() {
                core.enums[*id].name.clone()
            } else {
                format!(
                    "{}[{}]",
                    core.enums[*id].name,
                    args.iter()
                        .map(|&arg| type_name(core, arg))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        }
        _ => format!("type#{}", ty.index()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{lexer, modules::Module, parser};

    fn modules(sources: &[(&str, &str, bool)]) -> Modules {
        Modules {
            modules: sources
                .iter()
                .map(|&(name, text, builtin)| Module {
                    name: name.into(),
                    source_name: format!("{name}.emel"),
                    builtin,
                    ast: parser::parse(&lexer::lex(text).unwrap()).unwrap(),
                })
                .collect(),
            entry: 0,
        }
    }
    fn text(name: &str) -> ast::Text {
        ast::Text::new(name, Span::new(0, name.len()))
    }

    #[test]
    fn module_effects_are_nominal_and_generic_instances_reuse_only_matching_arguments() {
        let modules = modules(&[
            ("Main", "import Alpha\nimport Beta", false),
            (
                "Alpha",
                "pub effect Log { fn read()->Int }\npub effect State[S] { fn get()->S }",
                false,
            ),
            ("Beta", "pub effect Log { fn read()->Int }", false),
        ]);
        let mut core = b::Definitions::default();
        let mut registry = EffectRegistry::declare(&modules, &mut core).unwrap();
        let first = registry
            .resolve_instance(&modules, &mut core, 0, &text("Alpha.Log"), vec![])
            .unwrap();
        let other = registry
            .resolve_instance(&modules, &mut core, 0, &text("Beta.Log"), vec![])
            .unwrap();
        assert_ne!(first, other);
        assert_eq!(core.effects[first].decl, "Alpha.Log");
        let ints = registry
            .resolve_instance(
                &modules,
                &mut core,
                0,
                &text("Alpha.State"),
                vec![b::TypeId::INT],
            )
            .unwrap();
        let strings = registry
            .resolve_instance(
                &modules,
                &mut core,
                0,
                &text("Alpha.State"),
                vec![b::TypeId::STRING],
            )
            .unwrap();
        assert_ne!(ints, strings);
        assert_eq!(core.effects[ints].args, [b::TypeId::INT]);
        assert_eq!(core.effects[strings].name, "Alpha.State[String]");
        assert_eq!(
            ints,
            registry
                .resolve_instance(
                    &modules,
                    &mut core,
                    0,
                    &text("Alpha.State"),
                    vec![b::TypeId::INT]
                )
                .unwrap()
        );
        assert_eq!(registry.declaration(ints).unwrap().ast.name.value, "State");
    }

    #[test]
    fn effect_visibility_respects_selective_imports_qualified_names_and_prelude() {
        let modules = modules(&[
            ("Main", "import Alpha.{Visible}", false),
            (
                "Alpha",
                "pub effect Visible { fn read()->Int }\npub effect Unlisted { fn read()->Int }\neffect Hidden { fn read()->Int }",
                false,
            ),
            ("Prelude", "@intrinsic\npub effect Async {}", true),
        ]);
        let mut core = b::Definitions::default();
        let mut registry = EffectRegistry::declare(&modules, &mut core).unwrap();
        for name in [
            "Visible",
            "Alpha.Visible",
            "Alpha.Unlisted",
            "Async",
            "Prelude.Async",
        ] {
            registry
                .resolve_instance(&modules, &mut core, 0, &text(name), vec![])
                .unwrap();
        }
        for name in ["Hidden", "Alpha.Hidden", "Unlisted", "Unknown"] {
            let error = registry
                .resolve_instance(&modules, &mut core, 0, &text(name), vec![])
                .unwrap_err();
            assert_eq!(error.kind, DK::Name);
            assert_eq!(error.source.as_deref(), Some("Main.emel"));
        }
    }

    #[test]
    fn labels_reject_other_labels_builtin_effects_and_type_parameters() {
        let valid = modules(&[(
            "Main",
            "effect State[S] { fn get()->S }\neffect Hits of State[Int]\neffect Debug of State[Int] default",
            false,
        )]);
        EffectRegistry::declare(&valid, &mut b::Definitions::default()).unwrap();
        for source in [
            "effect Base {}\neffect Bad of Base",
            "effect Base { fn read()->Int }\neffect First of Base\neffect Bad of First",
            "effect Base { fn read()->Int }\neffect Bad[A] of Base",
            "@intrinsic\neffect Base { fn read()->Int }\neffect Bad of Base",
        ] {
            let modules = modules(&[("Main", source, true)]);
            let errors = EffectRegistry::declare(&modules, &mut b::Definitions::default())
                .err()
                .unwrap();
            assert!(
                errors.iter().any(|error| error.kind == DK::Type),
                "{source}: {errors:?}"
            );
        }
    }

    #[test]
    fn instances_reserved_before_signature_fill_receive_effect_and_operation_substitution() {
        let modules = modules(&[(
            "Std.Scope",
            "@intrinsic\npub effect Scope[S] { fn select[A](value:S,other:A)->A }",
            true,
        )]);
        let mut core = b::Definitions::default();
        let mut registry = EffectRegistry::declare(&modules, &mut core).unwrap();
        let id = registry
            .resolve_instance(&modules, &mut core, 0, &text("Scope"), vec![b::TypeId::INT])
            .unwrap();
        assert!(core.effects[id].ops.is_empty());
        let effect_param = core.types.intern(b::Type::Param(0));
        let op_param = core.types.intern(b::Type::Param(1));
        core.effects[registry.declarations[0].id].ops = vec![b::OpDef {
            name: "select".into(),
            suspend: false,
            type_params: 1,
            params: vec![effect_param, op_param],
            ret: op_param,
            fails: false,
        }];
        registry.refresh_instances(&mut core);
        let operation = &core.effects[id].ops[0];
        assert_eq!(operation.params, [b::TypeId::INT, effect_param]);
        assert_eq!(operation.ret, effect_param);
        assert_eq!(operation.type_params, 1);
    }

    #[test]
    fn effect_declarations_and_references_report_duplicates_arity_and_invalid_ids() {
        for source in [
            "effect Read {}\neffect Read {}",
            "type Read(value:Int)\neffect Read {}",
            "effect Read[A,A] {}",
            "effect Read { fn read()->Int\nfn read()->Int }",
            "effect Read { fn read(value:Int,value:Int)->Int }",
            "effect Read { fn read[A](value:A)->A }",
            "@intrinsic\neffect Read {}",
        ] {
            let modules = modules(&[("Main", source, false)]);
            let errors = EffectRegistry::declare(&modules, &mut b::Definitions::default())
                .err()
                .unwrap();
            assert!(
                errors
                    .iter()
                    .all(|error| error.source.as_deref() == Some("Main.emel")),
                "{source}: {errors:?}"
            );
            assert!(errors.iter().all(|error| error.span.end <= source.len()));
        }
        let modules = modules(&[("Main", "effect State[S] { fn get()->S }", false)]);
        let mut core = b::Definitions::default();
        let mut registry = EffectRegistry::declare(&modules, &mut core).unwrap();
        for args in [
            vec![],
            vec![b::TypeId::INT, b::TypeId::INT],
            vec![b::TypeId::from_index(9999)],
        ] {
            assert_eq!(
                registry
                    .resolve_instance(&modules, &mut core, 0, &text("State"), args)
                    .unwrap_err()
                    .kind,
                DK::Type
            );
        }
    }
}
