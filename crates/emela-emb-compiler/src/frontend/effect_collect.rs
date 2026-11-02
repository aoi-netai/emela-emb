//! Source effect identities and concrete instances, followed by handler/layer collection.
use super::{
    ast,
    data::{self, Catalog},
    ir::DefinitionId,
    modules::Modules,
    traits::ParamBounds,
    types::{InvocationSets, Signature},
};
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
    operation_errors: HashMap<b::EffectId, Vec<Vec<b::TypeId>>>,
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
        Ok(self.instance(core, index, args))
    }

    fn instance(
        &mut self,
        core: &mut b::Definitions,
        index: usize,
        args: Vec<b::TypeId>,
    ) -> b::EffectId {
        if let Some(&id) = self.instances.get(&(index, args.clone())) {
            return id;
        }
        let template_id = self.declarations[index].id;
        let template = core.effects[template_id].clone();
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
            ops: Vec::new(),
            label_of: template.label_of,
        });
        // Reserve before filling operations: a signature may recursively mention this instance.
        self.instances.insert((index, args.clone()), id);
        self.instance_declarations.insert(id, index);
        core.effects[id].ops = instantiate_operations(self, core, &template.ops, &args);
        if let Some(errors) = self.operation_errors.get(&template_id).cloned() {
            let concrete = instantiate_errors(self, core, &template.ops, &errors, &args);
            self.operation_errors.insert(id, concrete);
        }
        id
    }

    /// Source substitution must also rebuild a capability's nominal effect instance. A plain
    /// TypeTable substitution cannot inspect the type arguments carried behind an EffectId.
    pub fn substitute_type(
        &mut self,
        core: &mut b::Definitions,
        ty: b::TypeId,
        arguments: &[b::TypeId],
    ) -> b::TypeId {
        let substitutions = arguments
            .iter()
            .enumerate()
            .map(|(index, &argument)| (core.types.intern(b::Type::Param(index)), argument))
            .collect();
        self.substitute_map(core, ty, &substitutions)
    }

    /// Definition schemes use distinct TypeIds after their positional parameters have been
    /// scoped. Apply those substitutions simultaneously, including capability arguments.
    pub fn substitute_map(
        &mut self,
        core: &mut b::Definitions,
        ty: b::TypeId,
        substitutions: &HashMap<b::TypeId, b::TypeId>,
    ) -> b::TypeId {
        if let Some(&replacement) = substitutions.get(&ty) {
            return replacement;
        }
        let node = match core.ty(ty).clone() {
            b::Type::Handler(effect) => {
                let original = core.effects[effect].args.clone();
                let args = original
                    .iter()
                    .map(|&arg| self.substitute_map(core, arg, substitutions))
                    .collect::<Vec<_>>();
                if original == args {
                    return ty;
                }
                let index = self.instance_declarations[&effect];
                b::Type::Handler(self.instance(core, index, args))
            }
            b::Type::Tuple(fields) => b::Type::Tuple(
                fields
                    .into_iter()
                    .map(|field| self.substitute_map(core, field, substitutions))
                    .collect(),
            ),
            b::Type::Enum(id, fields) => b::Type::Enum(
                id,
                fields
                    .into_iter()
                    .map(|field| self.substitute_map(core, field, substitutions))
                    .collect(),
            ),
            b::Type::List(item) => b::Type::List(self.substitute_map(core, item, substitutions)),
            b::Type::Cell(item) => b::Type::Cell(self.substitute_map(core, item, substitutions)),
            b::Type::Fn(mut function) => {
                function.params = function
                    .params
                    .into_iter()
                    .map(|param| self.substitute_map(core, param, substitutions))
                    .collect();
                function.ret = self.substitute_map(core, function.ret, substitutions);
                if let Some(sets) = &mut function.sets {
                    sets.errors = self.substitute_map(core, sets.errors, substitutions);
                    sets.effects = self.substitute_map(core, sets.effects, substitutions);
                }
                b::Type::Fn(function)
            }
            b::Type::Set(mut row) => {
                let members = row
                    .params
                    .into_iter()
                    .map(|member| self.substitute_map(core, member, substitutions))
                    .collect::<Vec<_>>();
                row.params = Vec::new();
                for member in members {
                    match core.ty(member).clone() {
                        b::Type::Set(other) => row.merge(&other),
                        _ => row.params.push(member),
                    }
                }
                row.params.sort();
                row.params.dedup();
                b::Type::Set(row)
            }
            _ => return ty,
        };
        core.types.intern(node)
    }

    /// Signatures can mention capabilities before data fields and operations have been filled.
    /// Update those already-reserved instances once the template signatures are available.
    pub fn refresh_instances(&mut self, core: &mut b::Definitions) {
        let mut instances: Vec<_> = self
            .instances
            .iter()
            .map(|((index, args), &id)| (*index, args.clone(), id))
            .collect();
        instances.sort_by_key(|(_, _, id)| id.into_raw());
        for (index, args, id) in instances {
            let template = core.effects[self.declarations[index].id].clone();
            if id != self.declarations[index].id {
                core.effects[id].ops = instantiate_operations(self, core, &template.ops, &args);
                core.effects[id].label_of = template.label_of;
                if let Some(errors) = self
                    .operation_errors
                    .get(&self.declarations[index].id)
                    .cloned()
                {
                    let failures = instantiate_errors(self, core, &template.ops, &errors, &args);
                    self.operation_errors.insert(id, failures);
                }
            }
        }
    }

    pub fn operation_errors(&self, effect: b::EffectId, operation: usize) -> Option<&[b::TypeId]> {
        self.operation_errors
            .get(&effect)?
            .get(operation)
            .map(Vec::as_slice)
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
    registry: &mut EffectRegistry,
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
                    .map(|&ty| registry.substitute_type(core, ty, &substitution))
                    .collect(),
                ret: registry.substitute_type(core, operation.ret, &substitution),
                fails: operation.fails,
            }
        })
        .collect()
}

fn instantiate_errors(
    registry: &mut EffectRegistry,
    core: &mut b::Definitions,
    operations: &[b::OpDef],
    errors: &[Vec<b::TypeId>],
    args: &[b::TypeId],
) -> Vec<Vec<b::TypeId>> {
    errors
        .iter()
        .enumerate()
        .map(|(index, errors)| {
            let mut substitution = args.to_vec();
            substitution.extend(
                (0..operations[index].type_params)
                    .map(|index| core.types.intern(b::Type::Param(index))),
            );
            errors
                .iter()
                .map(|&error| registry.substitute_type(core, error, &substitution))
                .collect()
        })
        .collect()
}

/// Handler state has a nominal constructor, while the capability remains an unwritable type.
/// The checker restricts construction of this state to the matching handler's initializer.
pub fn expand_handlers(modules: &mut Modules) -> Result<(), Vec<Diagnostic>> {
    let mut additions = Vec::new();
    let mut errors = Vec::new();
    for (module, source) in modules.modules.iter().enumerate() {
        let mut names: HashSet<_> = source
            .ast
            .data
            .iter()
            .flat_map(|data| {
                std::iter::once(data.name.value.clone()).chain(
                    data.variants
                        .iter()
                        .map(|variant| variant.name.value.clone()),
                )
            })
            .chain(
                source
                    .ast
                    .effects
                    .iter()
                    .map(|item| item.name.value.clone()),
            )
            .chain(source.ast.traits.iter().map(|item| item.name.value.clone()))
            .chain(source.ast.layers.iter().map(|item| item.name.value.clone()))
            .collect();
        for handler in &source.ast.handlers {
            let result = (|| -> Result<(), Diagnostic> {
                if !names.insert(handler.name.value.clone()) {
                    return Err(Diagnostic::new(
                        DK::Name,
                        handler.name.span,
                        "duplicate handler/type declaration",
                    ));
                }
                if handler
                    .fields
                    .as_ref()
                    .is_some_and(|fields| !fields.is_empty())
                    && handler.init.is_none()
                {
                    return Err(Diagnostic::new(
                        DK::Type,
                        handler.name.span,
                        "a handler with state fields requires init",
                    ));
                }
                let mut fields = HashSet::new();
                for field in handler.fields.iter().flatten() {
                    let Some(name) = &field.name else {
                        return Err(Diagnostic::new(
                            DK::Type,
                            field.ty.span,
                            "handler fields require names",
                        ));
                    };
                    if !fields.insert(name.value.as_str()) {
                        return Err(Diagnostic::new(
                            DK::Name,
                            name.span,
                            "duplicate handler field",
                        ));
                    }
                }
                additions.push((
                    module,
                    ast::Data {
                        name: handler.name.clone(),
                        intrinsic: None,
                        public: handler.public,
                        opaque: false,
                        error: false,
                        params: vec![],
                        derives: vec![],
                        variants: vec![ast::Variant {
                            name: handler.name.clone(),
                            fields: handler.fields.clone().unwrap_or_default(),
                        }],
                    },
                ));
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
    for (module, data) in additions {
        modules.modules[module].ast.data.push(data);
    }
    Ok(())
}

/// Resolve operation value/error types only after nominal declarations are all registered.
pub fn fill_operations(modules: &Modules, catalog: &mut Catalog) -> Result<(), Vec<Diagnostic>> {
    let declarations = catalog.effects.declarations.clone();
    let mut errors = Vec::new();
    for declaration in declarations
        .iter()
        .filter(|declaration| declaration.ast.underlying.is_none())
    {
        let result = (|| -> Result<(), Diagnostic> {
            let mut operations = Vec::new();
            let mut failures = Vec::new();
            for operation in &declaration.ast.operations {
                let mut params: HashMap<_, _> = declaration
                    .ast
                    .params
                    .iter()
                    .enumerate()
                    .map(|(index, param)| (param.name.value.clone(), index))
                    .collect();
                for parameter in &operation.type_params {
                    let index = params.len();
                    if params.insert(parameter.name.value.clone(), index).is_some() {
                        return Err(Diagnostic::new(
                            DK::Name,
                            parameter.name.span,
                            "operation type parameter shadows an effect parameter",
                        ));
                    }
                }
                let arguments = operation
                    .params
                    .iter()
                    .map(|parameter| {
                        catalog
                            .resolve_type(modules, declaration.module, &parameter.ty, &params)
                            .map(data::backend_type)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let ret = match &operation.return_type {
                    Some(ty) => data::backend_type(catalog.resolve_type(
                        modules,
                        declaration.module,
                        ty,
                        &params,
                    )?),
                    None => b::TypeId::UNIT,
                };
                let mut errors = Vec::new();
                for error in operation.fails.iter().flatten() {
                    let ty = data::backend_type(catalog.resolve_type(
                        modules,
                        declaration.module,
                        error,
                        &params,
                    )?);
                    if !is_error(&catalog.core, ty) {
                        return Err(Diagnostic::new(
                            DK::Type,
                            error.span,
                            "operation fails clauses require error types",
                        ));
                    }
                    errors.push(ty);
                }
                errors.sort();
                errors.dedup();
                operations.push(b::OpDef {
                    name: operation.name.value.clone(),
                    suspend: operation.suspends,
                    type_params: operation.type_params.len(),
                    params: arguments,
                    ret,
                    fails: !errors.is_empty(),
                });
                failures.push(errors);
            }
            catalog.core.effects[declaration.id].ops = operations;
            catalog
                .effects
                .operation_errors
                .insert(declaration.id, failures);
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(error.in_source(&modules.modules[declaration.module].source_name));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    catalog.effects.refresh_instances(&mut catalog.core);
    for declaration in declarations
        .iter()
        .filter(|declaration| declaration.ast.underlying.is_some())
    {
        let target = declaration.ast.underlying.as_ref().unwrap();
        match catalog.resolve_effect(modules, declaration.module, target, &HashMap::new()) {
            Ok(effect) => {
                catalog.core.effects[declaration.id].label_of = Some(effect);
                catalog.core.effects[declaration.id].ops = catalog.core.effects[effect].ops.clone();
                if let Some(failures) = catalog.effects.operation_errors.get(&effect).cloned() {
                    catalog
                        .effects
                        .operation_errors
                        .insert(declaration.id, failures);
                }
            }
            Err(error) => {
                errors.push(error.in_source(&modules.modules[declaration.module].source_name))
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub(crate) type HandlerId = usize;

#[derive(Clone, Debug)]
pub(crate) enum MethodKind {
    Init(HandlerId),
    Release(HandlerId),
    Operation {
        handler: HandlerId,
        operation: usize,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct MethodInfo {
    pub signature: Signature,
    pub type_parameters: usize,
    pub self_type: Option<b::TypeId>,
    pub bounds: ParamBounds,
    pub errors_allowed: Option<Vec<b::TypeId>>,
    pub kind: MethodKind,
}
#[derive(Clone, Debug)]
pub(crate) struct HandlerInfo {
    pub id: HandlerId,
    pub module: usize,
    pub name: String,
    pub span: Span,
    pub state_type: b::TypeId,
    pub effect: b::EffectId,
    pub init: Option<DefinitionId>,
    pub release: Option<DefinitionId>,
    pub operations: Vec<DefinitionId>,
    pub fields_parenthesized: bool,
    pub requirements: Vec<b::EffectId>,
}
#[derive(Clone, Debug)]
pub(crate) struct LayerMember {
    pub handler: HandlerId,
    /// Resolve this initializer argument in its layer declaration, not its eventual caller.
    pub module: usize,
    pub args: Option<Vec<ast::Argument>>,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(crate) struct LayerInfo {
    pub id: usize,
    pub module: usize,
    pub name: String,
    pub span: Span,
    pub items: Vec<LayerMember>,
}
#[derive(Clone)]
struct Scope {
    name: String,
    builtin: bool,
    source_name: String,
    imports: Vec<ast::Import>,
    handlers: HashMap<String, (HandlerId, bool)>,
    layers: HashMap<String, (usize, bool)>,
}
struct PendingHandler {
    init: String,
    release: Option<String>,
    operations: Vec<String>,
}
pub(crate) struct EffectCollection {
    pub methods: HashMap<(usize, String), MethodInfo>,
    pub handlers: Vec<HandlerInfo>,
    pub layers: Vec<LayerInfo>,
    scopes: Vec<Scope>,
    pending: Vec<PendingHandler>,
}

impl EffectCollection {
    pub fn find_handler(&self, module: usize, name: &str) -> Option<HandlerId> {
        self.find_item(module, name, true)
    }
    pub fn find_layer(&self, module: usize, name: &str) -> Option<usize> {
        self.find_item(module, name, false)
    }
    pub fn handler_for_state(&self, state: b::TypeId) -> Option<&HandlerInfo> {
        self.handlers
            .iter()
            .find(|handler| handler.state_type == state)
    }
    fn find_item(&self, module: usize, name: &str, handler: bool) -> Option<usize> {
        let source = self.scopes.get(module)?;
        let table = |index: usize| {
            if handler {
                &self.scopes[index].handlers
            } else {
                &self.scopes[index].layers
            }
        };
        if let Some((prefix, member)) = name.rsplit_once('.') {
            let target = source
                .imports
                .iter()
                .find(|import| {
                    import.path.value == prefix
                        || import.path.value.rsplit('.').next() == Some(prefix)
                })
                .and_then(|import| {
                    self.scopes
                        .iter()
                        .position(|scope| scope.name == import.path.value)
                })
                .or_else(|| {
                    (prefix == "Prelude")
                        .then(|| {
                            self.scopes
                                .iter()
                                .position(|scope| scope.builtin && scope.name == "Prelude")
                        })
                        .flatten()
                })?;
            let &(id, public) = table(target).get(member)?;
            return public.then_some(id);
        }
        if let Some(&(id, _)) = table(module).get(name) {
            return Some(id);
        }
        let mut found = None;
        let mut explicit = false;
        for import in &source.imports {
            if !import
                .members
                .as_ref()
                .is_some_and(|items| items.iter().any(|item| item.value == name))
            {
                continue;
            }
            explicit = true;
            let target = self
                .scopes
                .iter()
                .position(|scope| scope.name == import.path.value)?;
            let &(id, public) = table(target).get(name)?;
            if !public || found.is_some_and(|previous| previous != id) {
                return None;
            }
            found = Some(id);
        }
        if explicit {
            return found;
        }
        let prelude = self
            .scopes
            .iter()
            .position(|scope| scope.builtin && scope.name == "Prelude")?;
        table(prelude)
            .get(name)
            .and_then(|&(id, public)| public.then_some(id))
    }
    fn diagnostic(&self, module: usize, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(DK::Type, span, message).in_source(&self.scopes[module].source_name)
    }
    fn add_method(
        &mut self,
        modules: &mut Modules,
        module: usize,
        mut function: ast::Function,
        info: MethodInfo,
    ) -> String {
        let suffix = match info.kind {
            MethodKind::Init(_) => "init".into(),
            MethodKind::Release(_) => "release".into(),
            MethodKind::Operation { operation, .. } => format!("op_{operation}"),
        };
        let handler = match info.kind {
            MethodKind::Init(id)
            | MethodKind::Release(id)
            | MethodKind::Operation { handler: id, .. } => id,
        };
        let name = format!("$handler_{handler}_{suffix}");
        function.name.value = name.clone();
        function.public = false;
        function.constant = false;
        function.attributes.clear();
        function.return_type = None;
        function.fails = None;
        function.effects = None;
        function.suspends = false;
        self.methods.insert((module, name.clone()), info);
        modules.modules[module].ast.functions.push(function);
        name
    }
    pub fn bind_functions(
        &mut self,
        functions: &HashMap<(usize, String), DefinitionId>,
    ) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let mut bound = Vec::new();
        for handler in &self.handlers {
            let pending = &self.pending[handler.id];
            let mut lookup = |name: &String| {
                functions
                    .get(&(handler.module, name.clone()))
                    .copied()
                    .ok_or_else(|| {
                        self.diagnostic(
                            handler.module,
                            handler.span,
                            "generated handler function has no resolved definition ID",
                        )
                    })
            };
            let result = (|| -> Result<_, Diagnostic> {
                let init = lookup(&pending.init)?;
                let release = pending.release.as_ref().map(&mut lookup).transpose()?;
                let operations = pending
                    .operations
                    .iter()
                    .map(&mut lookup)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((init, release, operations))
            })();
            match result {
                Ok(ids) => bound.push((handler.id, ids)),
                Err(error) => errors.push(error),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        for (handler, (init, release, operations)) in bound {
            self.handlers[handler].init = Some(init);
            self.handlers[handler].release = release;
            self.handlers[handler].operations = operations;
        }
        Ok(())
    }
}

fn function(span: Span, params: Vec<ast::Parameter>, body: ast::Block) -> ast::Function {
    ast::Function {
        name: ast::Text::new("", span),
        public: false,
        constant: false,
        type_params: vec![],
        params,
        return_type: None,
        fails: None,
        effects: None,
        suspends: false,
        attributes: vec![],
        body: Some(body),
    }
}
fn method_signature(params: Vec<b::TypeId>, ret: b::TypeId, errors: Vec<b::TypeId>) -> Signature {
    Signature {
        params: params.into_iter().map(data::frontend_type).collect(),
        ret: Some(data::frontend_type(ret)),
        sets: InvocationSets {
            errors,
            error_constraints: vec![],
            effects: vec![],
        },
    }
}
fn receiver(span: Span) -> ast::Parameter {
    ast::Parameter {
        name: ast::Text::new("self", span),
        ty: ast::Text::new("", span),
    }
}

pub fn prepare(
    modules: &mut Modules,
    catalog: &mut Catalog,
) -> Result<EffectCollection, Vec<Diagnostic>> {
    let mut collection = EffectCollection {
        methods: HashMap::new(),
        handlers: Vec::new(),
        layers: Vec::new(),
        pending: Vec::new(),
        scopes: modules
            .modules
            .iter()
            .map(|module| Scope {
                name: module.name.clone(),
                builtin: module.builtin,
                source_name: module.source_name.clone(),
                imports: module.ast.imports.clone(),
                handlers: HashMap::new(),
                layers: HashMap::new(),
            })
            .collect(),
    };
    let declarations: Vec<_> = modules
        .modules
        .iter()
        .enumerate()
        .flat_map(|(module, source)| {
            source
                .ast
                .handlers
                .iter()
                .cloned()
                .map(move |handler| (module, handler))
        })
        .collect();
    let layer_declarations: Vec<_> = modules
        .modules
        .iter()
        .enumerate()
        .flat_map(|(module, source)| {
            source
                .ast
                .layers
                .iter()
                .cloned()
                .map(move |layer| (module, layer))
        })
        .collect();
    let mut errors = Vec::new();
    for (module, handler) in declarations {
        let result = (|| -> Result<(), Diagnostic> {
            let id = collection.handlers.len();
            let effect =
                catalog.resolve_effect(modules, module, &handler.effect, &HashMap::new())?;
            let declaration = catalog.effects.declaration(effect).ok_or_else(|| {
                collection.diagnostic(
                    module,
                    handler.effect.span,
                    "handler effect has no source declaration",
                )
            })?;
            if declaration.ast.underlying.is_some() {
                return Err(collection.diagnostic(
                    module,
                    handler.effect.span,
                    "effect labels are supplied with by, not implemented by handlers",
                ));
            }
            if !declaration.ast.attributes.is_empty() {
                return Err(collection.diagnostic(
                    module,
                    handler.effect.span,
                    "compiler-owned intrinsic effects cannot have user handlers",
                ));
            }
            let state_enum = catalog
                .type_id(module, &handler.name.value)
                .ok_or_else(|| {
                    collection.diagnostic(
                        module,
                        handler.name.span,
                        "handler state was not registered before collecting its body",
                    )
                })?;
            let state_type = catalog.core.types.enum_type(state_enum, vec![]);
            collection.scopes[module]
                .handlers
                .insert(handler.name.value.clone(), (id, handler.public));
            collection.handlers.push(HandlerInfo {
                id,
                module,
                name: handler.name.value.clone(),
                span: handler.span,
                state_type,
                effect,
                init: None,
                release: None,
                operations: vec![],
                fields_parenthesized: handler.fields.is_some(),
                requirements: vec![],
            });
            let init_params = handler
                .init
                .as_ref()
                .map(|init| init.params.clone())
                .unwrap_or_default();
            let init_types = init_params
                .iter()
                .map(|param| {
                    catalog
                        .resolve_type(modules, module, &param.ty, &HashMap::new())
                        .map(data::backend_type)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let init_body = handler
                .init
                .as_ref()
                .map(|init| init.body.clone())
                .unwrap_or_else(|| {
                    let callee = ast::Expr {
                        kind: ast::ExprKind::Name(vec![handler.name.clone()]),
                        span: handler.name.span,
                    };
                    let kind = if handler.fields.is_some() {
                        ast::ExprKind::Call {
                            callee: Box::new(callee),
                            args: vec![],
                        }
                    } else {
                        callee.kind
                    };
                    ast::Block {
                        statements: vec![ast::Statement::Expr(ast::Expr {
                            kind,
                            span: handler.name.span,
                        })],
                        span: handler.name.span,
                    }
                });
            let init = collection.add_method(
                modules,
                module,
                function(handler.name.span, init_params, init_body),
                MethodInfo {
                    signature: method_signature(init_types, state_type, vec![]),
                    type_parameters: 0,
                    self_type: None,
                    bounds: HashMap::new(),
                    errors_allowed: None,
                    kind: MethodKind::Init(id),
                },
            );
            let release = handler.release.clone().map(|mut body| {
                body.statements.push(ast::Statement::Expr(ast::Expr {
                    kind: ast::ExprKind::Unit,
                    span: body.span,
                }));
                collection.add_method(
                    modules,
                    module,
                    function(body.span, vec![receiver(body.span)], body),
                    MethodInfo {
                        signature: method_signature(vec![state_type], b::TypeId::UNIT, vec![]),
                        type_parameters: 0,
                        self_type: Some(state_type),
                        bounds: HashMap::new(),
                        errors_allowed: Some(vec![]),
                        kind: MethodKind::Release(id),
                    },
                )
            });
            let expected = catalog.core.effects[effect].ops.clone();
            let mut written = HashMap::new();
            for operation in &handler.operations {
                if written
                    .insert(operation.name.value.as_str(), operation)
                    .is_some()
                {
                    return Err(collection.diagnostic(
                        module,
                        operation.name.span,
                        "duplicate handler operation",
                    ));
                }
                if !expected
                    .iter()
                    .any(|item| item.name == operation.name.value)
                {
                    return Err(collection.diagnostic(
                        module,
                        operation.name.span,
                        "handled effect has no such operation",
                    ));
                }
            }
            let missing: Vec<_> = expected
                .iter()
                .filter(|operation| !written.contains_key(operation.name.as_str()))
                .map(|operation| operation.name.as_str())
                .collect();
            if !missing.is_empty() {
                return Err(collection.diagnostic(
                    module,
                    Span::new(handler.span.start, handler.effect.span.end),
                    format!("handler is missing operations: {}", missing.join(", ")),
                ));
            }
            let mut operations = Vec::new();
            for (index, operation) in expected.iter().enumerate() {
                let implementation = written[operation.name.as_str()];
                if operation.suspend != implementation.suspends {
                    return Err(collection.diagnostic(
                        module,
                        implementation.name.span,
                        "handler operation suspend marker must match its effect declaration",
                    ));
                }
                let arity = operation.params.len() + usize::from(implementation.suspends);
                if arity != implementation.params.len() {
                    return Err(collection.diagnostic(
                        module,
                        implementation.name.span,
                        format!(
                            "handler operation expects {arity} parameters, found {}",
                            implementation.params.len()
                        ),
                    ));
                }
                if let Some(param) = implementation
                    .params
                    .iter()
                    .find(|param| !param.ty.value.is_empty())
                {
                    return Err(collection.diagnostic(
                        module,
                        param.ty.span,
                        "handler operation parameter types are inherited from the effect",
                    ));
                }
                let errors = catalog
                    .effects
                    .operation_errors(effect, index)
                    .ok_or_else(|| {
                        collection.diagnostic(
                            module,
                            implementation.name.span,
                            "operation error signature was not resolved before handler collection",
                        )
                    })?
                    .to_vec();
                if errors.iter().any(|&error| !is_error(&catalog.core, error)) {
                    return Err(collection.diagnostic(
                        module,
                        handler.effect.span,
                        "effect arguments used in fails must be single error types",
                    ));
                }
                let mut params = vec![state_type];
                params.extend(operation.params.iter().copied());
                let ret = if implementation.suspends {
                    params.push(catalog.core.types.intern(b::Type::Fn(b::FnType {
                        params: vec![operation.ret],
                        ret: b::TypeId::UNIT,
                        fails: false,
                        suspends: false,
                        ev: false,
                        sets: None,
                    })));
                    b::TypeId::UNIT
                } else {
                    operation.ret
                };
                let mut parameter_names = vec![receiver(implementation.name.span)];
                parameter_names.extend(implementation.params.clone());
                operations.push(collection.add_method(
                    modules,
                    module,
                    function(
                        implementation.name.span,
                        parameter_names,
                        implementation.body.clone(),
                    ),
                    MethodInfo {
                        signature: method_signature(params, ret, errors.clone()),
                        type_parameters: 0,
                        self_type: Some(state_type),
                        bounds: HashMap::new(),
                        errors_allowed: Some(errors),
                        kind: MethodKind::Operation {
                            handler: id,
                            operation: index,
                        },
                    },
                ));
            }
            collection.pending.push(PendingHandler {
                init,
                release,
                operations,
            });
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(error.in_source(&modules.modules[module].source_name));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    collect_layers(&mut collection, &layer_declarations, modules)?;
    Ok(collection)
}

fn layer_finish_order(edges: &[Vec<usize>]) -> Vec<usize> {
    let mut seen = vec![false; edges.len()];
    let mut order = Vec::new();
    for start in 0..edges.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if *next < edges[*node].len() {
                let child = edges[*node][*next];
                *next += 1;
                if !seen[child] {
                    seen[child] = true;
                    stack.push((child, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    order
}

fn collect_layers(
    collection: &mut EffectCollection,
    declarations: &[(usize, ast::Layer)],
    modules: &Modules,
) -> Result<(), Vec<Diagnostic>> {
    let mut errors = Vec::new();
    for (id, (module, layer)) in declarations.iter().enumerate() {
        let source = &modules.modules[*module];
        if collection.scopes[*module]
            .layers
            .contains_key(&layer.name.value)
            || collection.scopes[*module]
                .handlers
                .contains_key(&layer.name.value)
            || source
                .ast
                .data
                .iter()
                .any(|data| data.name.value == layer.name.value)
            || source
                .ast
                .traits
                .iter()
                .any(|item| item.name.value == layer.name.value)
            || source
                .ast
                .effects
                .iter()
                .any(|item| item.name.value == layer.name.value)
        {
            errors.push(
                Diagnostic::new(
                    DK::Name,
                    layer.name.span,
                    "duplicate layer/type declaration",
                )
                .in_source(&source.source_name),
            );
            continue;
        }
        collection.scopes[*module]
            .layers
            .insert(layer.name.value.clone(), (id, layer.public));
        collection.layers.push(LayerInfo {
            id,
            module: *module,
            name: layer.name.value.clone(),
            span: layer.span,
            items: vec![],
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    #[derive(Clone, Copy)]
    enum Target {
        Handler(usize),
        Layer(usize),
    }
    let mut targets = Vec::new();
    let mut edges = vec![vec![]; declarations.len()];
    for (id, (module, layer)) in declarations.iter().enumerate() {
        let mut resolved = Vec::new();
        for item in &layer.items {
            if item.by.is_some() {
                errors.push(collection.diagnostic(
                    *module,
                    item.span,
                    "layer items cannot contain by providers",
                ));
                continue;
            }
            if let Some(handler) = collection.find_handler(*module, &item.name.value) {
                resolved.push(Target::Handler(handler));
            } else if let Some(other) = collection.find_layer(*module, &item.name.value) {
                if item.args.is_some() {
                    errors.push(collection.diagnostic(
                        *module,
                        item.span,
                        "layers cannot take initialization arguments",
                    ));
                }
                resolved.push(Target::Layer(other));
                edges[id].push(other);
            } else {
                errors.push(
                    Diagnostic::new(
                        DK::Name,
                        item.name.span,
                        format!("unknown or private handler/layer '{}'", item.name.value),
                    )
                    .in_source(&modules.modules[*module].source_name),
                );
            }
        }
        targets.push(resolved);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let order = layer_finish_order(&edges);
    let mut reverse = vec![vec![]; edges.len()];
    for (from, children) in edges.iter().enumerate() {
        for &child in children {
            reverse[child].push(from);
        }
    }
    let mut seen = vec![false; edges.len()];
    for &start in order.iter().rev() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            component.push(node);
            for &parent in &reverse[node] {
                if !seen[parent] {
                    seen[parent] = true;
                    stack.push(parent);
                }
            }
        }
        if component.len() > 1 || edges[start].contains(&start) {
            component.sort();
            let first = component[0];
            let (module, layer) = &declarations[first];
            let names = component
                .iter()
                .map(|&id| declarations[id].1.name.value.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            errors.push(collection.diagnostic(
                *module,
                layer.name.span,
                format!("layers include one another: {names}"),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut invalid = HashSet::new();
    for id in order {
        let (module, layer) = &declarations[id];
        let mut effects = HashSet::new();
        let mut members = Vec::new();
        for (item, target) in layer.items.iter().zip(&targets[id]) {
            let additions = match *target {
                Target::Handler(handler) => vec![LayerMember {
                    handler,
                    module: *module,
                    args: item.args.clone(),
                    span: item.span,
                }],
                Target::Layer(other) => {
                    if invalid.contains(&other) {
                        invalid.insert(id);
                        continue;
                    }
                    collection.layers[other].items.clone()
                }
            };
            if let Some(duplicate) = additions
                .iter()
                .map(|member| collection.handlers[member.handler].effect)
                .find(|effect| !effects.insert(*effect))
            {
                let name = &collection
                    .handlers
                    .iter()
                    .find(|handler| handler.effect == duplicate)
                    .unwrap()
                    .name;
                errors.push(collection.diagnostic(
                    *module,
                    item.span,
                    format!("layer provides the same effect more than once (handler {name})"),
                ));
                invalid.insert(id);
            } else {
                members.extend(additions);
            }
        }
        collection.layers[id].items = members;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn is_error(core: &b::Definitions, ty: b::TypeId) -> bool {
    match core.ty(ty) {
        b::Type::Param(_) => true,
        b::Type::Enum(id, _) => core.enums[*id].error,
        _ => false,
    }
}

fn type_name(core: &b::Definitions, ty: b::TypeId) -> String {
    match core.ty(ty) {
        b::Type::Int => "Int".into(),
        b::Type::Int64 => "Int64".into(),
        b::Type::Float => "Float".into(),
        b::Type::Bool => "Bool".into(),
        b::Type::String => "String".into(),
        b::Type::Bytes => "Bytes".into(),
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
    fn collected(sources: &[(&str, &str, bool)]) -> (Modules, Catalog, EffectCollection) {
        let mut modules = modules(sources);
        expand_handlers(&mut modules).unwrap();
        let mut catalog = Catalog::collect(&modules).unwrap();
        fill_operations(&modules, &mut catalog).unwrap();
        let collection = prepare(&mut modules, &mut catalog).unwrap();
        (modules, catalog, collection)
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

    #[test]
    fn operation_fill_updates_early_capabilities_errors_and_labels() {
        let modules = modules(&[(
            "Main",
            "error Missing(value:Int)\neffect State[S,E] { fn get()->S fails E }\n\
             effect Hits of State[Int,Missing]\ntype Saved(cap:State[Int,Missing])",
            false,
        )]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        let saved = catalog.type_id(0, "Saved").unwrap();
        let cap = catalog.core.enums[saved].variants[0].tys[0];
        let b::Type::Handler(instance) = *catalog.core.ty(cap) else {
            panic!("capability has no nominal effect instance")
        };
        assert!(catalog.core.effects[instance].ops.is_empty());
        fill_operations(&modules, &mut catalog).unwrap();
        let operation = &catalog.core.effects[instance].ops[0];
        assert_eq!(operation.ret, b::TypeId::INT);
        assert!(operation.fails);
        let errors = catalog
            .effects
            .operation_errors(instance, 0)
            .unwrap()
            .to_vec();
        assert_eq!(errors.len(), 1);
        let b::Type::Enum(error, _) = catalog.core.ty(errors[0]) else {
            panic!("operation failure was not instantiated")
        };
        assert!(catalog.core.enums[*error].error);
        let label = catalog
            .resolve_effect(&modules, 0, &text("Hits"), &HashMap::new())
            .unwrap();
        assert_ne!(instance, label);
        assert_eq!(catalog.core.effects[label].label_of, Some(instance));
        assert_eq!(
            catalog.core.effects[label].ops,
            catalog.core.effects[instance].ops
        );
        assert_eq!(
            catalog.effects.operation_errors(label, 0),
            Some(errors.as_slice())
        );
    }

    #[test]
    fn operation_fill_rejects_value_failure_types_and_parameter_shadowing() {
        for (source, builtin) in [
            ("effect Bad { fn get()->Int fails Int }", false),
            ("@intrinsic\neffect Bad[A] { fn get[A](value:A)->A }", true),
            ("@intrinsic\neffect Bad { fn get[A,A](value:A)->A }", true),
        ] {
            let modules = modules(&[("Main", source, builtin)]);
            let mut catalog = Catalog::collect(&modules).unwrap();
            let errors = fill_operations(&modules, &mut catalog).unwrap_err();
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
            assert_eq!(errors[0].source.as_deref(), Some("Main.emel"));
        }
    }

    #[test]
    fn concrete_operation_callbacks_retain_their_failure_type_identities() {
        let modules = modules(&[(
            "Main",
            "error Missing\neffect Lift[E] { fn lift(callback:fn(Int)->Int fails E)->Int fails E }",
            false,
        )]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        fill_operations(&modules, &mut catalog).unwrap();
        let instance = catalog
            .resolve_effect(&modules, 0, &text("Lift[Missing]"), &HashMap::new())
            .unwrap();
        let callback = catalog.core.effects[instance].ops[0].params[0];
        let b::Type::Fn(callback) = catalog.core.ty(callback) else {
            panic!("operation callback is not a function type")
        };
        let b::Type::Set(errors) = catalog.core.ty(callback.sets.unwrap().errors) else {
            panic!("operation callback lost its source failure row")
        };
        assert_eq!(
            errors.params,
            catalog.effects.operation_errors(instance, 0).unwrap()
        );
        assert_eq!(errors.params.len(), 1);
    }

    #[test]
    fn effect_argument_substitution_rebuilds_nested_and_recursive_capabilities() {
        let modules = modules(&[(
            "Main",
            "effect State[S] { fn get()->S }\n\
             effect Carry[T] { fn carry(callback:fn(State[T])->State[T])->State[T] }\n\
             effect Loop[T] { fn next()->Loop[T] }",
            false,
        )]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        fill_operations(&modules, &mut catalog).unwrap();
        let carry = catalog
            .resolve_effect(&modules, 0, &text("Carry[Int]"), &HashMap::new())
            .unwrap();
        let state = catalog
            .resolve_effect(&modules, 0, &text("State[Int]"), &HashMap::new())
            .unwrap();
        let operation = &catalog.core.effects[carry].ops[0];
        assert_eq!(catalog.core.ty(operation.ret), &b::Type::Handler(state));
        let b::Type::Fn(callback) = catalog.core.ty(operation.params[0]) else {
            panic!("operation callback lost its function type")
        };
        assert_eq!(
            catalog.core.ty(callback.params[0]),
            &b::Type::Handler(state)
        );
        assert_eq!(catalog.core.ty(callback.ret), &b::Type::Handler(state));
        let recursive = catalog
            .resolve_effect(&modules, 0, &text("Loop[Int]"), &HashMap::new())
            .unwrap();
        assert_eq!(
            catalog.core.ty(catalog.core.effects[recursive].ops[0].ret),
            &b::Type::Handler(recursive)
        );
    }

    #[test]
    fn scoped_substitution_maps_capability_arguments_and_function_rows_simultaneously() {
        let modules = modules(&[("Main", "effect State[S] { fn get()->S }", false)]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        fill_operations(&modules, &mut catalog).unwrap();
        let scoped = catalog.core.types.intern(b::Type::Param(100));
        let unrelated = catalog.core.types.intern(b::Type::Param(101));
        let effect = catalog
            .effects
            .resolve_instance(&modules, &mut catalog.core, 0, &text("State"), vec![scoped])
            .unwrap();
        let capability = catalog.core.types.intern(b::Type::Handler(effect));
        let errors = catalog.core.types.intern(b::Type::Set(b::SetSummary {
            params: vec![unrelated],
            ..Default::default()
        }));
        let effects = catalog.core.types.intern(b::Type::Set(b::SetSummary {
            params: vec![capability],
            ..Default::default()
        }));
        let function = catalog.core.types.intern(b::Type::Fn(b::FnType {
            params: vec![capability],
            ret: scoped,
            fails: false,
            suspends: false,
            ev: true,
            sets: Some(b::FnSets { errors, effects }),
        }));
        let mapped = catalog.effects.substitute_map(
            &mut catalog.core,
            function,
            &HashMap::from([(scoped, unrelated), (unrelated, b::TypeId::INT)]),
        );
        let b::Type::Fn(mapped) = catalog.core.ty(mapped) else {
            panic!("function type changed")
        };
        assert_eq!(mapped.ret, unrelated);
        let b::Type::Handler(mapped_effect) = catalog.core.ty(mapped.params[0]) else {
            panic!("capability type changed")
        };
        assert_eq!(catalog.core.effects[*mapped_effect].args, [unrelated]);
        let sets = mapped.sets.unwrap();
        let b::Type::Set(mapped_errors) = catalog.core.ty(sets.errors) else {
            panic!("failure row changed")
        };
        assert_eq!(mapped_errors.params, [b::TypeId::INT]);
        let b::Type::Set(mapped_effects) = catalog.core.ty(sets.effects) else {
            panic!("effect row changed")
        };
        assert_eq!(mapped_effects.params, mapped.params);
    }

    #[test]
    fn handlers_generate_typed_init_release_and_suspend_implementation_functions() {
        let (modules, catalog, mut collection) = collected(&[(
            "Main",
            "error Missing\neffect Read { suspend fn read(delta:Int)->String fails Missing }\n\
             handler Reader(value:String): Read {\n\
               init(value:String) { Reader(value:value) }\n\
               release { 99 }\n\
               suspend fn read(delta,resume) { resume(self.value) }\n\
             }",
            false,
        )]);
        assert_eq!(collection.handlers.len(), 1);
        let state = collection.handlers[0].state_type;
        assert!(collection.handlers[0].fields_parenthesized);
        assert_eq!(modules.modules[0].ast.functions.len(), 3);
        let init = &collection.methods[&(0, "$handler_0_init".into())];
        assert_eq!(
            init.signature.params,
            [data::frontend_type(b::TypeId::STRING)]
        );
        assert_eq!(init.signature.ret, Some(data::frontend_type(state)));
        assert!(init.errors_allowed.is_none());
        assert!(matches!(init.kind, MethodKind::Init(0)));
        let release = &collection.methods[&(0, "$handler_0_release".into())];
        assert_eq!(release.self_type, Some(state));
        assert_eq!(release.errors_allowed, Some(vec![]));
        assert_eq!(
            release.signature.ret,
            Some(data::frontend_type(b::TypeId::UNIT))
        );
        let release_ast = modules.modules[0]
            .ast
            .functions
            .iter()
            .find(|function| function.name.value == "$handler_0_release")
            .unwrap();
        assert!(matches!(
            release_ast.body.as_ref().unwrap().statements.last(),
            Some(ast::Statement::Expr(ast::Expr {
                kind: ast::ExprKind::Unit,
                ..
            }))
        ));
        let operation = &collection.methods[&(0, "$handler_0_op_0".into())];
        assert_eq!(operation.signature.params.len(), 3);
        assert_eq!(operation.signature.params[0], data::frontend_type(state));
        assert_eq!(
            operation.signature.params[1],
            data::frontend_type(b::TypeId::INT)
        );
        let resume = data::backend_type(operation.signature.params[2]);
        let b::Type::Fn(resume) = catalog.core.ty(resume) else {
            panic!("suspend handler implementation has no resume parameter")
        };
        assert_eq!(resume.params, [b::TypeId::STRING]);
        assert_eq!(resume.ret, b::TypeId::UNIT);
        assert!(!resume.fails && !resume.suspends && !resume.ev);
        assert_eq!(
            operation.signature.ret,
            Some(data::frontend_type(b::TypeId::UNIT))
        );
        assert_eq!(operation.errors_allowed.as_ref().unwrap().len(), 1);
        let ids = modules.modules[0]
            .ast
            .functions
            .iter()
            .enumerate()
            .map(|(index, function)| ((0, function.name.value.clone()), DefinitionId(index)))
            .collect();
        collection.bind_functions(&ids).unwrap();
        assert_eq!(collection.handlers[0].init, Some(DefinitionId(0)));
        assert_eq!(collection.handlers[0].release, Some(DefinitionId(1)));
        assert_eq!(collection.handlers[0].operations, [DefinitionId(2)]);
        assert_eq!(collection.handler_for_state(state).unwrap().id, 0);
    }

    #[test]
    fn default_initializers_preserve_bare_and_parenthesized_constructor_forms() {
        let (modules, _, collection) = collected(&[(
            "Main",
            "effect Read { fn read()->Int }\n\
             handler Bare: Read { fn read() { 1 } }\n\
             handler Empty(): Read { fn read() { 2 } }",
            false,
        )]);
        assert!(!collection.handlers[0].fields_parenthesized);
        assert!(collection.handlers[1].fields_parenthesized);
        for (index, calls) in [(0, false), (1, true)] {
            let function = modules.modules[0]
                .ast
                .functions
                .iter()
                .find(|function| function.name.value == format!("$handler_{index}_init"))
                .unwrap();
            let ast::Statement::Expr(expression) = &function.body.as_ref().unwrap().statements[0]
            else {
                panic!("default initializer must return its constructor")
            };
            assert_eq!(matches!(expression.kind, ast::ExprKind::Call { .. }), calls);
        }
    }

    #[test]
    fn handlers_reject_missing_extra_duplicate_mismatched_and_ill_typed_operations() {
        for source in [
            "effect Read { fn read()->Int }\nhandler Bad: Read {}",
            "effect Read { fn read()->Int }\nhandler Bad: Read { fn nope() { 1 } }",
            "effect Read { fn read()->Int }\nhandler Bad: Read { fn read() { 1 }\nfn read() { 2 } }",
            "effect Read { fn read()->Int }\nhandler Bad: Read { suspend fn read(resume) { resume(1) } }",
            "effect Read { fn read(value:Int)->Int }\nhandler Bad: Read { fn read() { 1 } }",
            "effect Read { fn read(value:Int)->Int }\nhandler Bad: Read { fn read(value:Int) { value } }",
            "effect Read[E] { fn read()->Int fails E }\nhandler Bad: Read[Int] { fn read() { 1 } }",
            "effect Read { fn read()->Int }\neffect Label of Read\nhandler Bad: Label { fn read() { 1 } }",
        ] {
            let mut modules = modules(&[("Main", source, false)]);
            expand_handlers(&mut modules).unwrap();
            let mut catalog = Catalog::collect(&modules).unwrap();
            fill_operations(&modules, &mut catalog).unwrap();
            let errors = prepare(&mut modules, &mut catalog).err().unwrap();
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
            assert_eq!(errors[0].source.as_deref(), Some("Main.emel"));
        }
        let mut modules = modules(&[(
            "Main",
            "effect Read { fn read()->Int }\nhandler Bad(value:Int): Read { fn read() { 1 } }",
            false,
        )]);
        let errors = expand_handlers(&mut modules).unwrap_err();
        assert!(errors[0].message.contains("requires init"));
    }

    #[test]
    fn layers_flatten_in_source_order_and_preserve_argument_declaration_scope() {
        let (_, _, collection) = collected(&[
            ("Main", "import Alpha\nlayer App { Alpha.Bundle }", false),
            (
                "Alpha",
                "effect State[S] { fn get()->S }\n\
                 handler Numbers(value:Int): State[Int] { init(value:Int) { Numbers(value:value) }\nfn get() { self.value } }\n\
                 handler Words: State[String] { fn get() { \"ok\" } }\n\
                 pub layer Bundle { Numbers(value:42)\nWords }",
                false,
            ),
        ]);
        let app = collection.find_layer(0, "App").unwrap();
        let items = &collection.layers[app].items;
        assert_eq!(items.len(), 2);
        assert_eq!(collection.handlers[items[0].handler].name, "Numbers");
        assert_eq!(collection.handlers[items[1].handler].name, "Words");
        assert_eq!(items[0].module, 1);
        assert!(items[0].args.as_ref().is_some_and(|args| args.len() == 1));
        assert!(items[1].args.is_none());
        assert!(collection.find_handler(0, "Alpha.Numbers").is_none());
        assert!(collection.find_layer(0, "Alpha.Bundle").is_some());
    }

    #[test]
    fn layers_reject_cycles_repeated_effects_and_layer_initializer_syntax() {
        for layer in [
            "layer Bad { Bad }",
            "layer Bad { Other }\nlayer Other { Bad }",
            "layer Inner { One }\nlayer Bad { One\nInner }",
            "layer Inner { One }\nlayer Bad { Inner() }",
            "layer Bad { Hidden }",
        ] {
            let source = format!(
                "effect Read {{ fn read()->Int }}\n\
                 handler One: Read {{ fn read() {{ 1 }} }}\n\
                 handler Two: Read {{ fn read() {{ 2 }} }}\n{layer}"
            );
            let mut modules = modules(&[("Main", &source, false)]);
            expand_handlers(&mut modules).unwrap();
            let mut catalog = Catalog::collect(&modules).unwrap();
            fill_operations(&modules, &mut catalog).unwrap();
            let errors = prepare(&mut modules, &mut catalog).err().unwrap();
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        }
    }

    #[test]
    fn function_binding_is_transactional_and_reports_the_handler_source() {
        let (_, _, mut collection) = collected(&[(
            "Main",
            "effect Read { fn read()->Int }\nhandler One: Read { fn read() { 1 } }",
            false,
        )]);
        let errors = collection.bind_functions(&HashMap::new()).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].source.as_deref(), Some("Main.emel"));
        assert!(collection.handlers[0].init.is_none());
        assert!(collection.handlers[0].operations.is_empty());
    }
}
