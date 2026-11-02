//! Source trait collection and generated method signatures over the shared type catalog.
use super::{
    ast,
    data::{self, Catalog},
    ir::DefinitionId,
    modules::Modules,
    traits::{BuiltinTrait, Impl, Method, ParamBounds, Registry, Trait, TraitId, TypeHead},
    types::{InvocationSets, Signature},
};
use crate::{Diagnostic, DiagnosticKind as DK, Span, contract::ir as b};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub(crate) struct MethodInfo {
    pub signature: Signature,
    pub type_parameters: usize,
    pub derivation: Option<BuiltinTrait>,
    pub bounds: ParamBounds,
    pub errors_allowed: Vec<b::TypeId>,
    pub self_type: Option<b::TypeId>,
}
#[derive(Clone)]
struct VisibleTrait {
    id: TraitId,
    public: bool,
}
#[derive(Clone)]
struct Scope {
    name: String,
    source_name: String,
    imports: Vec<ast::Import>,
    names: HashMap<String, VisibleTrait>,
}
#[derive(Clone)]
struct MethodTemplate {
    name: String,
    signature: Signature,
    function: Option<(usize, String)>,
}
struct TraitTemplate {
    id: TraitId,
    methods: Vec<MethodTemplate>,
}
struct PendingImpl {
    head: TypeHead,
    trait_id: TraitId,
    owner: usize,
    module: usize,
    span: Span,
    bounds: Vec<Vec<TraitId>>,
    methods: HashMap<String, (usize, String)>,
    derived_fields: Option<Vec<b::TypeId>>,
}
pub(crate) struct TraitCollection {
    pub registry: Registry,
    pub methods: HashMap<(usize, String), MethodInfo>,
    pub builtin_traits: HashMap<BuiltinTrait, TraitId>,
    scopes: Vec<Scope>,
    prelude: usize,
    templates: Vec<TraitTemplate>,
    implementations: Vec<PendingImpl>,
    builtin_functions: Vec<(BuiltinTrait, String, (usize, String))>,
}

fn diagnostic(module: &Scope, span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(DK::Type, span, message).in_source(&module.source_name)
}
fn method_name(kind: BuiltinTrait) -> &'static str {
    match kind {
        BuiltinTrait::Eq => "eq",
        BuiltinTrait::Ord => "compare",
        BuiltinTrait::Show => "show",
    }
}
fn trait_name(kind: BuiltinTrait) -> &'static str {
    match kind {
        BuiltinTrait::Eq => "Eq",
        BuiltinTrait::Ord => "Ord",
        BuiltinTrait::Show => "Show",
    }
}
fn function_type(core: &mut b::Definitions, signature: &Signature) -> b::TypeId {
    let errors = core.types.intern(b::Type::Set(b::SetSummary {
        fails: !signature.sets.errors.is_empty(),
        params: signature.sets.errors.clone(),
        ..b::SetSummary::default()
    }));
    let effects = core.types.intern(b::Type::Set(b::SetSummary::default()));
    core.types.intern(b::Type::Fn(b::FnType {
        params: signature
            .params
            .iter()
            .copied()
            .map(data::backend_type)
            .collect(),
        ret: data::backend_type(signature.ret.expect("trait returns are explicit or Unit")),
        fails: !signature.sets.errors.is_empty(),
        suspends: false,
        ev: false,
        sets: Some(b::FnSets { errors, effects }),
    }))
}
fn substitute_signature(
    core: &mut b::Definitions,
    original: &Signature,
    self_type: b::TypeId,
) -> Signature {
    let params = original
        .params
        .iter()
        .map(|ty| data::frontend_type(core.types.subst(data::backend_type(*ty), &[self_type])))
        .collect();
    let ret = original
        .ret
        .map(|ty| data::frontend_type(core.types.subst(data::backend_type(ty), &[self_type])));
    let errors = original
        .sets
        .errors
        .iter()
        .map(|ty| core.types.subst(*ty, &[self_type]))
        .collect::<Vec<_>>();
    let constraints = original
        .sets
        .error_constraints
        .iter()
        .map(|ty| core.types.subst(*ty, &[self_type]))
        .collect();
    Signature {
        params,
        ret,
        sets: InvocationSets {
            errors,
            error_constraints: constraints,
            effects: vec![],
        },
    }
}
fn synthetic_function(
    name: String,
    span: Span,
    params: usize,
    parameter_names: &[String],
) -> ast::Function {
    ast::Function {
        name: ast::Text::new(name, span),
        public: false,
        constant: false,
        type_params: (0..params)
            .map(|index| ast::TypeParameter {
                name: ast::Text::new(format!("T{index}"), span),
                bounds: vec![],
            })
            .collect(),
        params: parameter_names
            .iter()
            .map(|name| ast::Parameter {
                name: ast::Text::new(name, span),
                ty: ast::Text::new("", span),
            })
            .collect(),
        return_type: None,
        fails: None,
        attributes: vec![],
        body: None,
    }
}

impl TraitCollection {
    pub fn find_trait(&self, module: usize, path: &str) -> Option<TraitId> {
        let scope = self.scopes.get(module)?;
        if let Some((alias, name)) = path.split_once('.') {
            let target = scope
                .imports
                .iter()
                .find(|import| {
                    import.members.is_none() && import.path.value.rsplit('.').next() == Some(alias)
                })
                .and_then(|import| {
                    self.scopes
                        .iter()
                        .position(|scope| scope.name == import.path.value)
                })
                .or_else(|| (alias == "Prelude").then_some(self.prelude))?;
            let item = self.scopes[target].names.get(name)?;
            return (target == module || item.public).then_some(item.id);
        }
        if let Some(item) = scope.names.get(path) {
            return Some(item.id);
        }
        let mut found = None;
        let mut explicitly_imported = false;
        for import in &scope.imports {
            if !import
                .members
                .as_ref()
                .is_some_and(|members| members.iter().any(|member| member.value == path))
            {
                continue;
            }
            explicitly_imported = true;
            let target = self
                .scopes
                .iter()
                .position(|scope| scope.name == import.path.value)?;
            if let Some(item) = self.scopes[target]
                .names
                .get(path)
                .filter(|item| item.public)
            {
                if found.is_some_and(|previous| previous != item.id) {
                    return None;
                }
                found = Some(item.id);
            } else {
                return None;
            }
        }
        if explicitly_imported {
            return found;
        }
        found.or_else(|| {
            self.scopes[self.prelude]
                .names
                .get(path)
                .filter(|item| item.public)
                .map(|item| item.id)
        })
    }
    fn require_trait(&self, module: usize, name: &ast::Text) -> Result<TraitId, Diagnostic> {
        self.find_trait(module, &name.value).ok_or_else(|| {
            Diagnostic::new(
                DK::Name,
                name.span,
                format!("undefined or inaccessible trait '{}'", name.value),
            )
            .in_source(&self.scopes[module].source_name)
        })
    }
    fn bounds(
        &self,
        module: usize,
        parameters: &[ast::TypeParameter],
    ) -> Result<Vec<Vec<TraitId>>, Diagnostic> {
        parameters
            .iter()
            .map(|parameter| {
                parameter
                    .bounds
                    .iter()
                    .map(|bound| self.require_trait(module, bound))
                    .collect()
            })
            .collect()
    }
    fn add_function(
        &mut self,
        modules: &mut Modules,
        module: usize,
        mut function: ast::Function,
        info: MethodInfo,
    ) -> (usize, String) {
        // '$' cannot be spelled by a source identifier, keeping generated methods private
        // even from other declarations in their defining module.
        let name = format!("$trait_{}_{}", self.methods.len(), function.name.value);
        function.name.value = name.clone();
        function.public = false;
        function.attributes.clear();
        let key = (module, name);
        self.methods.insert(key.clone(), info);
        modules.modules[module].ast.functions.push(function);
        key
    }
    /// Validate instance requirements without publishing synthetic DefinitionIds.
    /// Registry coherence and bound checks use method presence, not executable bodies.
    fn preview_registry(&self, core: &mut b::Definitions) -> Result<Registry, Diagnostic> {
        let mut registry = self.registry.clone();
        for template in &self.templates {
            let methods = template
                .methods
                .iter()
                .map(|method| Method {
                    name: method.name.clone(),
                    signature: function_type(core, &method.signature),
                    default_function: method.function.as_ref().map(|_| DefinitionId(0)),
                })
                .collect();
            registry
                .set_methods(template.id, methods)
                .map_err(|error| Diagnostic::new(DK::Type, Span::new(0, 0), error.message))?;
        }
        for implementation in &self.implementations {
            registry
                .add_impl(
                    core,
                    Impl {
                        head: implementation.head,
                        trait_id: implementation.trait_id,
                        owner_module: implementation.owner,
                        type_parameters: implementation.bounds.clone(),
                        methods: implementation
                            .methods
                            .keys()
                            .map(|name| (name.clone(), DefinitionId(0)))
                            .collect(),
                    },
                )
                .map_err(|error| {
                    diagnostic(
                        &self.scopes[implementation.module],
                        implementation.span,
                        error.message,
                    )
                })?;
        }
        Ok(registry)
    }
    fn validate_instances(&mut self, core: &mut b::Definitions) -> Result<(), Diagnostic> {
        let registry = self.preview_registry(core)?;
        for implementation in &self.implementations {
            let bounds = parameter_bounds(core, &implementation.bounds);
            let target = match implementation.head {
                TypeHead::Enum(id) => {
                    let args = (0..implementation.bounds.len())
                        .map(|index| core.types.intern(b::Type::Param(index)))
                        .collect();
                    core.types.enum_type(id, args)
                }
                TypeHead::Primitive(kind) => match kind {
                    super::traits::PrimitiveType::Int => b::TypeId::INT,
                    super::traits::PrimitiveType::Int64 => b::TypeId::INT64,
                    super::traits::PrimitiveType::Float => b::TypeId::FLOAT,
                    super::traits::PrimitiveType::Bool => b::TypeId::BOOL,
                    super::traits::PrimitiveType::String => b::TypeId::STRING,
                    super::traits::PrimitiveType::Unit => b::TypeId::UNIT,
                    super::traits::PrimitiveType::Never => b::TypeId::NEVER,
                },
            };
            if !registry
                .satisfies(core, target, implementation.trait_id, &bounds)
                .map_err(|error| {
                    diagnostic(
                        &self.scopes[implementation.module],
                        implementation.span,
                        error.message,
                    )
                })?
            {
                return Err(diagnostic(
                    &self.scopes[implementation.module],
                    implementation.span,
                    "impl does not satisfy its bounds and supertraits",
                ));
            }
            if let Some(fields) = &implementation.derived_fields {
                for &field in fields {
                    if !registry
                        .satisfies(core, field, implementation.trait_id, &bounds)
                        .map_err(|error| {
                            diagnostic(
                                &self.scopes[implementation.module],
                                implementation.span,
                                error.message,
                            )
                        })?
                    {
                        return Err(diagnostic(
                            &self.scopes[implementation.module],
                            implementation.span,
                            "derived trait is not implemented by a field type",
                        ));
                    }
                }
                for key in implementation.methods.values() {
                    self.methods
                        .get_mut(key)
                        .expect("generated derive signature")
                        .bounds = bounds.clone();
                }
            }
        }
        Ok(())
    }
    pub fn bind_functions(
        &mut self,
        core: &mut b::Definitions,
        definitions: &HashMap<(usize, String), DefinitionId>,
    ) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        for template in &self.templates {
            let mut methods = Vec::new();
            for method in &template.methods {
                let default_function = method
                    .function
                    .as_ref()
                    .and_then(|key| definitions.get(key).copied());
                if method.function.is_some() && default_function.is_none() {
                    errors.push(diagnostic(
                        &self.scopes[self
                            .registry
                            .trait_data(template.id)
                            .expect("registered trait")
                            .owner_module],
                        Span::new(0, 0),
                        "generated default method was not collected",
                    ));
                }
                methods.push(Method {
                    name: method.name.clone(),
                    signature: function_type(core, &method.signature),
                    default_function,
                });
            }
            if let Err(error) = self.registry.set_methods(template.id, methods) {
                errors.push(Diagnostic::new(DK::Type, Span::new(0, 0), error.message));
            }
        }
        for (kind, name, key) in &self.builtin_functions {
            if let Some(&definition) = definitions.get(key) {
                self.registry
                    .register_builtin_method(*kind, name, definition);
            } else {
                errors.push(diagnostic(
                    &self.scopes[key.0],
                    Span::new(0, 0),
                    "generated builtin method was not collected",
                ));
            }
        }
        for implementation in &self.implementations {
            let mut methods = HashMap::new();
            for (name, key) in &implementation.methods {
                if let Some(&definition) = definitions.get(key) {
                    methods.insert(name.clone(), definition);
                } else {
                    errors.push(diagnostic(
                        &self.scopes[implementation.module],
                        implementation.span,
                        "generated impl method was not collected",
                    ));
                }
            }
            if let Err(error) = self.registry.add_impl(
                core,
                Impl {
                    head: implementation.head,
                    trait_id: implementation.trait_id,
                    owner_module: implementation.owner,
                    type_parameters: implementation.bounds.clone(),
                    methods,
                },
            ) {
                errors.push(diagnostic(
                    &self.scopes[implementation.module],
                    implementation.span,
                    error.message,
                ));
            }
        }
        if errors.is_empty() {
            if let Err(error) = self.registry.validate_impls(core) {
                errors.push(Diagnostic::new(DK::Type, Span::new(0, 0), error.message));
            }
            for implementation in &self.implementations {
                let Some(fields) = &implementation.derived_fields else {
                    continue;
                };
                let bounds: ParamBounds = implementation
                    .bounds
                    .iter()
                    .enumerate()
                    .map(|(index, bounds)| {
                        (core.types.intern(b::Type::Param(index)), bounds.clone())
                    })
                    .collect();
                for &field in fields {
                    match self
                        .registry
                        .satisfies(core, field, implementation.trait_id, &bounds)
                    {
                        Ok(true) => {}
                        Ok(false) => errors.push(diagnostic(
                            &self.scopes[implementation.module],
                            implementation.span,
                            "derived trait is not implemented by a field type",
                        )),
                        Err(error) => errors.push(diagnostic(
                            &self.scopes[implementation.module],
                            implementation.span,
                            error.message,
                        )),
                    }
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

pub(crate) fn prepare(
    modules: &mut Modules,
    catalog: &mut Catalog,
) -> Result<TraitCollection, Vec<Diagnostic>> {
    prepare_inner(modules, catalog).map_err(|error| vec![error])
}
fn prepare_inner(
    modules: &mut Modules,
    catalog: &mut Catalog,
) -> Result<TraitCollection, Diagnostic> {
    let prelude = modules
        .modules
        .iter()
        .position(|module| module.builtin && module.name == "Prelude")
        .ok_or_else(|| {
            Diagnostic::new(
                DK::Backend,
                Span::new(0, 0),
                "compiler-owned Prelude is missing",
            )
        })?;
    let mut collection = TraitCollection {
        registry: Registry::new(),
        methods: HashMap::new(),
        builtin_traits: HashMap::new(),
        scopes: modules
            .modules
            .iter()
            .map(|module| Scope {
                name: module.name.clone(),
                source_name: module.source_name.clone(),
                imports: module.ast.imports.clone(),
                names: HashMap::new(),
            })
            .collect(),
        prelude,
        templates: vec![],
        implementations: vec![],
        builtin_functions: vec![],
    };
    for declaration in &catalog.declarations {
        collection
            .registry
            .register_type_owner(declaration.id, declaration.module)
            .map_err(|error| {
                diagnostic(
                    &collection.scopes[declaration.module],
                    declaration.ast.name.span,
                    error.message,
                )
            })?;
    }
    let option = catalog.type_id(prelude, "Option").ok_or_else(|| {
        diagnostic(
            &collection.scopes[prelude],
            Span::new(0, 0),
            "compiler Prelude has no Option declaration",
        )
    })?;
    collection
        .registry
        .register_option_type(&catalog.core, option)
        .map_err(|error| diagnostic(&collection.scopes[prelude], Span::new(0, 0), error.message))?;
    let ordering = catalog
        .resolve_type(
            modules,
            prelude,
            &ast::Text::new("Ordering", Span::new(0, 0)),
            &HashMap::new(),
        )
        .map_err(|error| error.in_source(&collection.scopes[prelude].source_name))?;
    let self_type = catalog.core.types.intern(b::Type::Param(0));
    for kind in [BuiltinTrait::Eq, BuiltinTrait::Ord, BuiltinTrait::Show] {
        let name = trait_name(kind);
        let id = collection
            .registry
            .add_trait(Trait {
                name: name.into(),
                owner_module: prelude,
                supers: vec![],
                methods: vec![],
            })
            .map_err(|error| {
                diagnostic(&collection.scopes[prelude], Span::new(0, 0), error.message)
            })?;
        collection.registry.set_builtin(id, kind).map_err(|error| {
            diagnostic(&collection.scopes[prelude], Span::new(0, 0), error.message)
        })?;
        collection.builtin_traits.insert(kind, id);
        collection.scopes[prelude]
            .names
            .insert(name.into(), VisibleTrait { id, public: true });
        let signature = Signature {
            params: vec![
                data::frontend_type(self_type);
                if kind == BuiltinTrait::Show { 1 } else { 2 }
            ],
            ret: Some(match kind {
                BuiltinTrait::Eq => data::frontend_type(b::TypeId::BOOL),
                BuiltinTrait::Ord => ordering,
                BuiltinTrait::Show => data::frontend_type(b::TypeId::STRING),
            }),
            sets: InvocationSets::default(),
        };
        let function = synthetic_function(
            method_name(kind).into(),
            Span::new(0, 0),
            1,
            &if kind == BuiltinTrait::Show {
                vec!["self".into()]
            } else {
                vec!["self".into(), "other".into()]
            },
        );
        let key = collection.add_function(
            modules,
            prelude,
            function,
            MethodInfo {
                signature: signature.clone(),
                type_parameters: 1,
                derivation: Some(kind),
                bounds: HashMap::from([(self_type, vec![id])]),
                errors_allowed: vec![],
                self_type: Some(self_type),
            },
        );
        collection
            .builtin_functions
            .push((kind, method_name(kind).into(), key));
        collection
            .registry
            .set_methods(
                id,
                vec![Method {
                    name: method_name(kind).into(),
                    signature: function_type(&mut catalog.core, &signature),
                    default_function: None,
                }],
            )
            .map_err(|error| {
                diagnostic(&collection.scopes[prelude], Span::new(0, 0), error.message)
            })?;
    }
    let eq = collection.builtin_traits[&BuiltinTrait::Eq];
    let ord = collection.builtin_traits[&BuiltinTrait::Ord];
    collection
        .registry
        .set_supers(ord, vec![eq])
        .expect("compiler-owned Ord extends Eq");
    let declarations = modules
        .modules
        .iter()
        .enumerate()
        .flat_map(|(module, data)| {
            data.ast
                .traits
                .iter()
                .cloned()
                .map(move |item| (module, item))
        })
        .collect::<Vec<_>>();
    for (module, declaration) in &declarations {
        let id = collection
            .registry
            .add_trait(Trait {
                name: declaration.name.value.clone(),
                owner_module: *module,
                supers: vec![],
                methods: vec![],
            })
            .map_err(|error| {
                diagnostic(
                    &collection.scopes[*module],
                    declaration.name.span,
                    error.message,
                )
            })?;
        collection.scopes[*module].names.insert(
            declaration.name.value.clone(),
            VisibleTrait {
                id,
                public: declaration.public,
            },
        );
    }
    let mut super_edges = HashMap::from([(ord, vec![eq])]);
    for (module, declaration) in &declarations {
        let id = collection.scopes[*module].names[&declaration.name.value].id;
        super_edges.insert(
            id,
            declaration
                .supers
                .iter()
                .map(|parent| collection.require_trait(*module, parent))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if let Some(cycle) = supertrait_cycle(&super_edges) {
        let first = collection
            .registry
            .trait_data(cycle[0])
            .expect("registered cycle trait");
        let declaration = declarations
            .iter()
            .find(|(module, declaration)| {
                *module == first.owner_module && declaration.name.value == first.name
            })
            .expect("source cycle declaration");
        let names = cycle
            .iter()
            .map(|&id| {
                collection
                    .registry
                    .trait_data(id)
                    .expect("registered trait")
                    .name
                    .as_str()
            })
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(diagnostic(
            &collection.scopes[first.owner_module],
            declaration.1.name.span,
            format!("supertrait cycle: {names}"),
        ));
    }
    for (module, declaration) in declarations {
        let id = collection.scopes[module].names[&declaration.name.value].id;
        let supers = super_edges.remove(&id).unwrap_or_default();
        collection
            .registry
            .set_supers(id, supers)
            .map_err(|error| {
                diagnostic(
                    &collection.scopes[module],
                    declaration.name.span,
                    error.message,
                )
            })?;
        let mut methods = Vec::new();
        let mut names = HashSet::new();
        for method in declaration.methods {
            if !names.insert(method.name.value.clone()) {
                return Err(diagnostic(
                    &collection.scopes[module],
                    method.name.span,
                    "duplicate trait method",
                ));
            }
            if !method.type_params.is_empty() || !method.attributes.is_empty() {
                return Err(diagnostic(
                    &collection.scopes[module],
                    method.name.span,
                    "trait methods cannot introduce type parameters or attributes",
                ));
            }
            let signature = method_signature(modules, catalog, module, &method)?;
            let errors = signature.sets.errors.clone();
            let function = if method.body.is_some() {
                let mut default = method.clone();
                default.type_params = vec![ast::TypeParameter {
                    name: ast::Text::new("Self", method.name.span),
                    bounds: vec![declaration.name.clone()],
                }];
                Some(collection.add_function(
                    modules,
                    module,
                    default,
                    MethodInfo {
                        signature: signature.clone(),
                        type_parameters: 1,
                        derivation: None,
                        bounds: HashMap::from([(self_type, vec![id])]),
                        errors_allowed: errors.clone(),
                        self_type: Some(self_type),
                    },
                ))
            } else {
                None
            };
            methods.push(MethodTemplate {
                name: method.name.value,
                signature,
                function,
            });
        }
        let registry_methods = methods
            .iter()
            .map(|method| Method {
                name: method.name.clone(),
                signature: function_type(&mut catalog.core, &method.signature),
                default_function: None,
            })
            .collect();
        collection
            .registry
            .set_methods(id, registry_methods)
            .map_err(|error| {
                diagnostic(
                    &collection.scopes[module],
                    declaration.name.span,
                    error.message,
                )
            })?;
        collection.templates.push(TraitTemplate { id, methods });
    }
    let implementations = modules
        .modules
        .iter()
        .enumerate()
        .flat_map(|(module, data)| {
            data.ast
                .impls
                .iter()
                .cloned()
                .map(move |item| (module, item))
        })
        .collect::<Vec<_>>();
    for (module, implementation) in implementations {
        let head_span = Span::new(
            implementation.span.start,
            implementation.trait_name.span.end,
        );
        let trait_id = collection.require_trait(module, &implementation.trait_name)?;
        let mut parameter_map = HashMap::new();
        for (index, parameter) in implementation.params.iter().enumerate() {
            if parameter_map
                .insert(parameter.name.value.clone(), index)
                .is_some()
            {
                return Err(diagnostic(
                    &collection.scopes[module],
                    head_span,
                    "duplicate impl type parameter",
                ));
            }
            if existing_type(modules, catalog, module, &parameter.name.value) {
                return Err(diagnostic(
                    &collection.scopes[module],
                    head_span,
                    "impl head parameter conflicts with an existing type",
                ));
            }
        }
        let target = data::backend_type(
            catalog
                .resolve_type(modules, module, &implementation.target, &parameter_map)
                .map_err(|error| error.in_source(&collection.scopes[module].source_name))?,
        );
        let head = Registry::head_of(&catalog.core, target)
            .map_err(|error| {
                diagnostic(
                    &collection.scopes[module],
                    implementation.target.span,
                    error.message,
                )
            })?
            .ok_or_else(|| {
                diagnostic(
                    &collection.scopes[module],
                    implementation.target.span,
                    "impl target must be a primitive or nominal data type",
                )
            })?;
        let required = collection.bounds(module, &implementation.params)?;
        let bounds = parameter_bounds(&mut catalog.core, &required);
        let mut methods = HashMap::new();
        let trait_data = collection
            .registry
            .trait_data(trait_id)
            .expect("resolved trait")
            .clone();
        for method in implementation.methods {
            if methods.contains_key(&method.name.value) {
                return Err(diagnostic(
                    &collection.scopes[module],
                    method.name.span,
                    "duplicate impl method",
                ));
            }
            let expected = trait_data
                .methods
                .iter()
                .find(|item| item.name == method.name.value)
                .ok_or_else(|| {
                    diagnostic(
                        &collection.scopes[module],
                        method.name.span,
                        "implemented trait has no such method",
                    )
                })?;
            let b::Type::Fn(expected_type) = catalog.core.ty(expected.signature) else {
                unreachable!("trait method signature")
            };
            if expected_type.params.len() != method.params.len() {
                return Err(diagnostic(
                    &collection.scopes[module],
                    method.name.span,
                    "impl method parameter count differs from trait signature",
                ));
            }
            if !method.type_params.is_empty()
                || method.return_type.is_some()
                || method.fails.is_some()
                || !method.attributes.is_empty()
                || method.params.iter().any(|param| {
                    !param.ty.value.is_empty()
                        && !(param.name.value == "self"
                            && param.ty.value == "Self"
                            && param.ty.span == param.name.span)
                })
            {
                return Err(diagnostic(
                    &collection.scopes[module],
                    method.name.span,
                    "impl method types and failure sets are inherited from the trait",
                ));
            }
            let original = signature_from_type(&catalog.core, expected.signature);
            let signature = substitute_signature(&mut catalog.core, &original, target);
            let errors = signature.sets.errors.clone();
            let mut generated = method.clone();
            generated.type_params = implementation.params.clone();
            let key = collection.add_function(
                modules,
                module,
                generated,
                MethodInfo {
                    signature,
                    type_parameters: implementation.params.len(),
                    derivation: None,
                    bounds: bounds.clone(),
                    errors_allowed: errors,
                    self_type: Some(target),
                },
            );
            methods.insert(method.name.value, key);
        }
        collection.implementations.push(PendingImpl {
            head,
            trait_id,
            owner: if modules.modules[module].builtin {
                prelude
            } else {
                module
            },
            module,
            span: head_span,
            bounds: required,
            methods,
            derived_fields: None,
        });
    }
    for declaration in catalog.declarations.clone() {
        let module = declaration.module;
        let fields = catalog.core.enums[declaration.id]
            .variants
            .iter()
            .flat_map(|variant| variant.tys.iter().copied())
            .collect::<Vec<_>>();
        let required_base = collection.bounds(module, &declaration.ast.params)?;
        let mut seen = HashSet::new();
        for derive in &declaration.ast.derives {
            let id = collection.require_trait(module, derive)?;
            let kind = collection
                .builtin_traits
                .iter()
                .find_map(|(kind, &candidate)| (candidate == id).then_some(*kind))
                .ok_or_else(|| {
                    diagnostic(
                        &collection.scopes[module],
                        derive.span,
                        "only Eq, Ord and Show can be derived",
                    )
                })?;
            if !seen.insert(id) {
                return Err(diagnostic(
                    &collection.scopes[module],
                    derive.span,
                    "duplicate derived trait",
                ));
            }
            let mut required = required_base.clone();
            // Spec 10.5 requires the derived bound on every parameter, including phantom parameters.
            for bounds in &mut required {
                if !bounds.contains(&id) {
                    bounds.push(id);
                }
            }
            let args = (0..declaration.ast.params.len())
                .map(|index| catalog.core.types.intern(b::Type::Param(index)))
                .collect();
            let target = catalog.core.types.enum_type(declaration.id, args);
            let original = signature_from_type(
                &catalog.core,
                collection
                    .registry
                    .trait_data(id)
                    .expect("builtin trait")
                    .methods[0]
                    .signature,
            );
            let signature = substitute_signature(&mut catalog.core, &original, target);
            let mut function = synthetic_function(
                method_name(kind).into(),
                derive.span,
                declaration.ast.params.len(),
                &if kind == BuiltinTrait::Show {
                    vec!["self".into()]
                } else {
                    vec!["self".into(), "other".into()]
                },
            );
            function.type_params = declaration.ast.params.clone();
            let key = collection.add_function(
                modules,
                module,
                function,
                MethodInfo {
                    signature,
                    type_parameters: declaration.ast.params.len(),
                    derivation: Some(kind),
                    bounds: parameter_bounds(&mut catalog.core, &required),
                    errors_allowed: vec![],
                    self_type: Some(target),
                },
            );
            collection.implementations.push(PendingImpl {
                head: TypeHead::Enum(declaration.id),
                trait_id: id,
                owner: module,
                module,
                span: derive.span,
                bounds: required,
                methods: HashMap::from([(method_name(kind).into(), key)]),
                derived_fields: Some(fields.clone()),
            });
        }
    }
    collection.validate_instances(&mut catalog.core)?;
    Ok(collection)
}
fn parameter_bounds(core: &mut b::Definitions, required: &[Vec<TraitId>]) -> ParamBounds {
    required
        .iter()
        .enumerate()
        .map(|(index, bounds)| (core.types.intern(b::Type::Param(index)), bounds.clone()))
        .collect()
}
fn method_signature(
    modules: &Modules,
    catalog: &mut Catalog,
    module: usize,
    method: &ast::Function,
) -> Result<Signature, Diagnostic> {
    let parameters = HashMap::from([("Self".into(), 0)]);
    let params = method
        .params
        .iter()
        .map(|parameter| catalog.resolve_type(modules, module, &parameter.ty, &parameters))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.in_source(&modules.modules[module].source_name))?;
    let ret = method
        .return_type
        .as_ref()
        .map(|ty| catalog.resolve_type(modules, module, ty, &parameters))
        .transpose()
        .map_err(|error| error.in_source(&modules.modules[module].source_name))?
        .unwrap_or(data::frontend_type(b::TypeId::UNIT));
    let errors = method
        .fails
        .as_ref()
        .into_iter()
        .flatten()
        .map(|ty| {
            catalog
                .resolve_type(modules, module, ty, &parameters)
                .map(data::backend_type)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.in_source(&modules.modules[module].source_name))?;
    let mut constraints = Vec::new();
    for &ty in &errors {
        match catalog.core.ty(ty) {
            b::Type::Param(_) => constraints.push(ty),
            b::Type::Enum(id, _) if catalog.core.enums[*id].error => {}
            _ => {
                return Err(Diagnostic::new(
                    DK::Type,
                    method.name.span,
                    "trait failure set requires an error type",
                )
                .in_source(&modules.modules[module].source_name));
            }
        }
    }
    Ok(Signature {
        params,
        ret: Some(ret),
        sets: InvocationSets {
            errors,
            error_constraints: constraints,
            effects: vec![],
        },
    })
}
fn signature_from_type(core: &b::Definitions, ty: b::TypeId) -> Signature {
    let b::Type::Fn(signature) = core.ty(ty) else {
        unreachable!("registered method signature")
    };
    let errors = signature
        .sets
        .and_then(|sets| match core.ty(sets.errors) {
            b::Type::Set(errors) => Some(errors.params.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let constraints = errors
        .iter()
        .filter(|&&ty| matches!(core.ty(ty), b::Type::Param(_)))
        .copied()
        .collect();
    Signature {
        params: signature
            .params
            .iter()
            .copied()
            .map(data::frontend_type)
            .collect(),
        ret: Some(data::frontend_type(signature.ret)),
        sets: InvocationSets {
            errors,
            error_constraints: constraints,
            effects: vec![],
        },
    }
}
fn existing_type(modules: &Modules, catalog: &Catalog, module: usize, name: &str) -> bool {
    if matches!(
        name,
        "Int" | "Int64" | "Float" | "Bool" | "String" | "Unit" | "Never" | "Self"
    ) {
        return true;
    }
    if catalog.type_id(module, name).is_some() {
        return true;
    }
    if modules
        .modules
        .iter()
        .enumerate()
        .any(|(index, module)| module.builtin && catalog.type_id(index, name).is_some())
    {
        return true;
    }
    modules.modules[module].ast.imports.iter().any(|import| {
        import
            .members
            .as_ref()
            .is_some_and(|members| members.iter().any(|member| member.value == name))
            && modules.modules.iter().enumerate().any(|(index, module)| {
                module.name == import.path.value && catalog.type_id(index, name).is_some()
            })
    })
}

fn supertrait_cycle(edges: &HashMap<TraitId, Vec<TraitId>>) -> Option<Vec<TraitId>> {
    let mut nodes = edges.keys().copied().collect::<Vec<_>>();
    nodes.sort_unstable();
    let mut finished = HashSet::new();
    for start in nodes {
        if finished.contains(&start) {
            continue;
        }
        let mut path = vec![start];
        let mut positions = HashMap::from([(start, 0)]);
        let mut stack = vec![(start, 0)];
        while let Some((node, next)) = stack.last_mut() {
            let children = edges.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if *next == children.len() {
                finished.insert(*node);
                positions.remove(node);
                stack.pop();
                path.pop();
                continue;
            }
            let child = children[*next];
            *next += 1;
            if let Some(&position) = positions.get(&child) {
                let mut cycle = path[position..].to_vec();
                let first = cycle
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, id)| *id)
                    .map(|(index, _)| index)
                    .unwrap();
                cycle.rotate_left(first);
                cycle.push(cycle[0]);
                return Some(cycle);
            }
            if !finished.contains(&child) {
                positions.insert(child, path.len());
                path.push(child);
                stack.push((child, 0));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::{lexer, modules::Module, parser};
    fn modules(source: &str, others: &[(&str, &str)]) -> Modules {
        let mut entries = vec![("Main", "main.emel", false, source)];
        entries.extend(
            others
                .iter()
                .map(|(name, source)| (*name, "other.emel", false, *source)),
        );
        entries.push(("Prelude", "<core/Prelude>", true, "pub enum Option[A] { Some(A), None }\npub enum Ordering { Less, Equal, Greater }\npub error ParseError(input: String)"));
        Modules {
            entry: 0,
            modules: entries
                .into_iter()
                .map(|(name, source_name, builtin, text)| Module {
                    name: name.into(),
                    source_name: source_name.into(),
                    builtin,
                    ast: parser::parse(&lexer::lex(text).unwrap()).unwrap(),
                })
                .collect(),
        }
    }
    fn definitions(collection: &TraitCollection) -> HashMap<(usize, String), DefinitionId> {
        let mut keys = collection.methods.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys.into_iter()
            .enumerate()
            .map(|(index, key)| (key, DefinitionId(index)))
            .collect()
    }
    #[test]
    fn custom_methods_defaults_and_builtins_bind_only_real_definition_ids() {
        let mut modules = modules(
            "type Key(value: Int)\ntrait Score { fn score(self) -> Int\n fn next(self) -> Int { Score.score(self) + 1 } }\nimpl Key: Score { fn score(self) { self.value + 1 } }",
            &[],
        );
        let mut catalog = Catalog::collect(&modules).unwrap();
        let mut collection = prepare(&mut modules, &mut catalog).unwrap();
        let ids = definitions(&collection);
        collection.bind_functions(&mut catalog.core, &ids).unwrap();
        let key = catalog
            .core
            .types
            .enum_type(catalog.type_id(0, "Key").unwrap(), vec![]);
        let score = collection.find_trait(0, "Score").unwrap();
        let next = collection
            .registry
            .resolve(&catalog.core, key, score, "next", &HashMap::new())
            .unwrap();
        assert!(next.from_default);
        assert_eq!(next.type_arguments, vec![key]);
        assert!(ids.values().any(|id| Some(*id) == next.definition));
        let score = collection
            .registry
            .resolve(&catalog.core, key, score, "score", &HashMap::new())
            .unwrap();
        assert!(!score.from_default);
        let info = collection
            .methods
            .iter()
            .find_map(|(name, info)| (ids[name] == score.definition.unwrap()).then_some(info))
            .unwrap();
        assert_eq!(info.self_type, Some(key));
        let default = modules.modules[0]
            .ast
            .functions
            .iter()
            .find(|function| function.name.value.contains("next"))
            .unwrap();
        assert_eq!(default.type_params[0].name.value, "Self");
        assert!(
            lexer::lex(&default.name.value).is_err(),
            "generated methods must not be source-spellable"
        );
        let float_ord = collection
            .registry
            .satisfies(
                &catalog.core,
                b::TypeId::FLOAT,
                collection.builtin_traits[&BuiltinTrait::Ord],
                &HashMap::new(),
            )
            .unwrap();
        assert!(!float_ord);
    }
    #[test]
    fn generic_type_names_are_parameters_but_existing_types_cannot_be_impl_parameters() {
        let source = "type Box[Value](value: Value)\nimpl Box[Item: Eq]: Eq { fn eq(self, other) { Eq.eq(self.value, other.value) } }";
        let mut modules = modules(source, &[]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        let mut collection = prepare(&mut modules, &mut catalog).unwrap();
        let ids = definitions(&collection);
        collection.bind_functions(&mut catalog.core, &ids).unwrap();
        let box_id = catalog.type_id(0, "Box").unwrap();
        let ty = catalog.core.types.enum_type(box_id, vec![b::TypeId::INT]);
        assert!(
            collection
                .registry
                .satisfies(
                    &catalog.core,
                    ty,
                    collection.builtin_traits[&BuiltinTrait::Eq],
                    &HashMap::new()
                )
                .unwrap()
        );
        for head in ["Box[Int]", "Pair[A,A]"] {
            let source = format!(
                "type Box[Value](value: Value)\ntype Pair[A,B](a:A,b:B)\nimpl {head}: Eq {{ fn eq(self, other) {{ True }} }}"
            );
            let mut modules = super::tests::modules(&source, &[]);
            let mut catalog = Catalog::collect(&modules).unwrap();
            let error = prepare(&mut modules, &mut catalog)
                .err()
                .expect("invalid impl head")
                .remove(0);
            assert_eq!(
                &source[error.span.start..error.span.end],
                format!("impl {head}: Eq")
            );
        }
    }
    #[test]
    fn cycles_orphans_overlaps_and_missing_methods_have_source_locations() {
        for source in [
            "trait First: Second {}\ntrait Second: First {}",
            "type Key\ntrait Score { fn score(self) -> Int }\nimpl Key: Score {}",
            "impl Int: Show { fn show(self) { \"n\" } }",
            "type Key\nimpl Key: Show { fn show(self) { \"a\" } }\nimpl Key: Show { fn show(self) { \"b\" } }",
        ] {
            let mut modules = modules(source, &[]);
            let mut catalog = Catalog::collect(&modules).unwrap();
            let error = prepare(&mut modules, &mut catalog)
                .err()
                .expect(source)
                .remove(0);
            assert_eq!(error.source.as_deref(), Some("main.emel"));
            assert!(error.span.start < error.span.end && error.span.end <= source.len());
            if source.starts_with("trait First") {
                assert_eq!(&source[error.span.start..error.span.end], "First");
                assert!(error.message.contains("First -> Second -> First"));
            }
        }
    }
    #[test]
    fn imported_traits_respect_visibility_and_builtin_identity() {
        let mut modules = modules(
            "import Traits.{Visible, Hidden}\n",
            &[("Traits", "pub trait Visible {}\ntrait Hidden {}")],
        );
        let mut catalog = Catalog::collect(&modules).unwrap();
        let collection = prepare(&mut modules, &mut catalog).unwrap();
        assert!(collection.find_trait(0, "Visible").is_some());
        assert!(collection.find_trait(0, "Hidden").is_none());
        assert_eq!(
            collection.find_trait(0, "Ord"),
            Some(collection.builtin_traits[&BuiltinTrait::Ord])
        );
    }
    #[test]
    fn derived_bounds_include_phantom_parameters_and_float_fields_reject_ord() {
        let mut modules = modules("type Phantom[Item]\ntype FloatBox(value: Float)", &[]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        // This is a catalog-level fixture. Source activation delegates derive validation
        // to this collector after Catalog has allocated the nominal identities.
        catalog.declarations[0].ast.derives = vec![ast::Text::new("Eq", Span::new(5, 12))];
        let mut collection = prepare(&mut modules, &mut catalog).unwrap();
        let ids = definitions(&collection);
        collection.bind_functions(&mut catalog.core, &ids).unwrap();
        let function = catalog.core.types.intern(b::Type::Fn(b::FnType {
            params: vec![],
            ret: b::TypeId::INT,
            fails: false,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let phantom = catalog
            .core
            .types
            .enum_type(catalog.type_id(0, "Phantom").unwrap(), vec![function]);
        assert!(
            !collection
                .registry
                .satisfies(
                    &catalog.core,
                    phantom,
                    collection.builtin_traits[&BuiltinTrait::Eq],
                    &HashMap::new()
                )
                .unwrap()
        );
        let mut modules = super::tests::modules("type FloatBox(value: Float)", &[]);
        let mut catalog = Catalog::collect(&modules).unwrap();
        catalog.declarations[0].ast.derives = vec![
            ast::Text::new("Eq", Span::new(5, 13)),
            ast::Text::new("Ord", Span::new(5, 13)),
        ];
        let error = prepare(&mut modules, &mut catalog)
            .err()
            .expect("Float has no Ord")
            .remove(0);
        assert!(error.message.contains("field type"));
        assert_eq!(error.span, Span::new(5, 13));
    }
}
