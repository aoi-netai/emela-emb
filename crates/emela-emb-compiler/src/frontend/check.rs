mod methods;
mod trait_methods;
use super::{
    ast,
    data::{self, Catalog},
    ir::{self, DefinitionId, Local, LocalId},
    modules::Modules,
    types::{InvocationSets, Signature, ValueType as Ty},
};
use crate::{
    Diagnostic, DiagnosticKind as DK, Span,
    sources::{self, Intrinsic},
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct Definition {
    module: usize,
    ast: ast::Function,
    signature: Signature,
    name: String,
    exported: bool,
    export_symbol: Option<String>,
    external: Option<String>,
    intrinsic: Option<Intrinsic>,
    error_mode: super::errors::Mode,
    bounds: super::traits::ParamBounds,
    self_type: Option<crate::contract::ir::TypeId>,
    derivation: Option<super::traits::BuiltinTrait>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Symbol {
    Function(DefinitionId),
    Constant(ConstantId),
    Constructor(crate::contract::ir::CtorRef),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstantId {
    IntMin,
    IntMax,
    Int64Min,
    Int64Max,
}
impl ConstantId {
    fn ty(self) -> Ty {
        match self {
            Self::IntMin | Self::IntMax => Ty::Int,
            Self::Int64Min | Self::Int64Max => Ty::Int64,
        }
    }
    fn literal(self) -> ir::Literal {
        match self {
            Self::IntMin => ir::Literal::Int(i32::MIN),
            Self::IntMax => ir::Literal::Int(i32::MAX),
            Self::Int64Min => ir::Literal::Int64(i64::MIN),
            Self::Int64Max => ir::Literal::Int64(i64::MAX),
        }
    }
}
#[derive(Default)]
struct Scope {
    values: HashMap<String, Symbol>,
    modules: HashMap<String, usize>,
}
struct Checker {
    modules: Modules,
    catalog: Catalog,
    traits: super::trait_collect::TraitCollection,
    current_bounds: super::traits::ParamBounds,
    current_self_type: Option<crate::contract::ir::TypeId>,
    trait_obligations: Vec<trait_methods::Obligation>,
    solver: super::infer::Solver,
    current_parameters: HashMap<String, usize>,
    definitions: Vec<Definition>,
    module_definitions: Vec<HashMap<String, DefinitionId>>,
    scopes: Vec<Scope>,
    checked: Vec<Option<ir::Function>>,
    states: Vec<u8>,
    schemes: super::schemes::Schemes,
}
struct Locals {
    names: HashMap<String, Local>,
    next: usize,
}
impl Locals {
    fn bind(&mut self, name: String, ty: Ty) -> Local {
        let local = Local {
            id: LocalId(self.next),
            name: name.clone(),
            ty,
        };
        self.next += 1;
        if name != "_" {
            self.names.insert(name, local.clone());
        }
        local
    }
}

pub(crate) fn check(mut modules: Modules) -> Result<ir::Program, Vec<Diagnostic>> {
    let mut catalog = Catalog::collect(&modules)?;
    let traits = super::trait_collect::prepare(&mut modules, &mut catalog)?;
    let mut checker = Checker {
        catalog,
        traits,
        current_bounds: Default::default(),
        current_self_type: None,
        trait_obligations: Vec::new(),
        solver: super::infer::Solver::new(),
        current_parameters: HashMap::new(),
        module_definitions: vec![HashMap::new(); modules.modules.len()],
        scopes: (0..modules.modules.len())
            .map(|_| Scope::default())
            .collect(),
        modules,
        definitions: Vec::new(),
        checked: Vec::new(),
        states: Vec::new(),
        schemes: super::schemes::Schemes::default(),
    };
    checker.collect()?;
    let ids = checker
        .definitions
        .iter()
        .enumerate()
        .map(|(i, d)| ((d.module, d.ast.name.value.clone()), DefinitionId(i)))
        .collect();
    checker
        .traits
        .bind_functions(&mut checker.catalog.core, &ids)?;
    checker.imports()?;
    checker.validate_data_declarations().map_err(|e| vec![e])?;
    checker
        .validate_method_declarations()
        .map_err(|e| vec![e])?;
    for definition in &checker.definitions {
        for ty in definition
            .signature
            .params
            .iter()
            .chain(definition.signature.ret.iter())
        {
            checker
                .validate_type_bounds(
                    definition.module,
                    data::backend_type(*ty),
                    &definition.bounds,
                    definition.ast.name.span,
                )
                .map_err(|e| vec![e])?;
        }
    }

    checker.checked.resize(checker.definitions.len(), None);
    checker.states.resize(checker.definitions.len(), 0);
    checker.schemes.initialize(
        checker
            .definitions
            .iter()
            .map(|definition| {
                (0..definition.ast.type_params.len())
                    .map(|index| {
                        checker
                            .catalog
                            .core
                            .types
                            .intern(crate::contract::ir::Type::Param(index))
                    })
                    .collect()
            })
            .collect(),
    );
    let mut errors = Vec::new();
    for index in 0..checker.definitions.len() {
        if let Err(error) = checker.function(DefinitionId(index)) {
            errors.push(error);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    checker.validate_trait_obligations()?;
    let modes: HashMap<_, _> = checker
        .definitions
        .iter()
        .enumerate()
        .map(|(i, d)| (DefinitionId(i), d.error_mode.clone()))
        .collect();
    let mut program = ir::Program {
        core: checker.catalog.core.clone(),
        registry: checker.traits.registry.clone(),
        functions: checker.checked.into_iter().flatten().collect(),
    };
    super::normalize::normalize(
        &mut program,
        &mut checker.solver,
        &checker.schemes.parameters,
    )?;
    checker.catalog.core = program.core.clone();
    super::errors::infer(&mut program, &modes, &mut checker.catalog)?;
    Ok(program)
}
impl Checker {
    fn collect(&mut self) -> Result<(), Vec<Diagnostic>> {
        let mut errors = Vec::new();
        let mut exports = HashSet::new();
        let entry = &self.modules.modules[self.modules.entry];
        if entry.ast.functions.is_empty()
            && entry.ast.imports.is_empty()
            && entry.ast.data.is_empty()
        {
            return Err(vec![
                Diagnostic::new(
                    DK::Syntax,
                    Span::new(0, 0),
                    "entry source contains no declarations",
                )
                .in_source(&entry.source_name),
            ]);
        }
        for module_index in 0..self.modules.modules.len() {
            let module = self.modules.modules[module_index].clone();
            for function in &module.ast.functions {
                let result = (|| -> Result<(), Diagnostic> {
                    let method = self
                        .traits
                        .methods
                        .get(&(module_index, function.name.value.clone()))
                        .cloned();
                    if method
                        .as_ref()
                        .is_some_and(|method| method.type_parameters != function.type_params.len())
                    {
                        return Err(Diagnostic::new(
                            DK::Type,
                            function.name.span,
                            "method type parameter metadata is inconsistent",
                        ));
                    }
                    let mut bounds = method
                        .as_ref()
                        .map(|m| m.bounds.clone())
                        .unwrap_or_default();
                    let mut type_parameters = HashMap::new();
                    for (index, parameter) in function.type_params.iter().enumerate() {
                        let mut constraints = Vec::new();
                        for bound in &parameter.bounds {
                            constraints.push(
                                self.traits
                                    .find_trait(module_index, &bound.value)
                                    .ok_or_else(|| {
                                        Diagnostic::new(
                                            DK::Name,
                                            bound.span,
                                            format!("undefined trait '{}'", bound.value),
                                        )
                                    })?,
                            );
                        }
                        if !constraints.is_empty() {
                            let ty = self
                                .catalog
                                .core
                                .types
                                .intern(crate::contract::ir::Type::Param(index));
                            bounds.entry(ty).or_default().extend(constraints);
                        }
                        if !(parameter.name.value.as_bytes()[0].is_ascii_uppercase()
                            || (method.is_some() && parameter.name.value.starts_with('$')))
                            || type_parameters
                                .insert(parameter.name.value.clone(), index)
                                .is_some()
                        {
                            return Err(Diagnostic::new(
                                DK::Name,
                                parameter.name.span,
                                "invalid or duplicate type parameter",
                            ));
                        }
                        self.solver
                            .register_rigid_parameter(&mut self.catalog.core.types, index)
                            .map_err(|e| {
                                Diagnostic::new(DK::Type, parameter.name.span, e.message)
                            })?;
                    }
                    if function.name.value == "_" {
                        return Err(Diagnostic::new(
                            DK::Name,
                            function.name.span,
                            "a function cannot be named '_'",
                        ));
                    }
                    let mut names = HashSet::new();
                    let mut params = Vec::new();
                    for (parameter_index, param) in function.params.iter().enumerate() {
                        if param.name.value != "_" && !names.insert(param.name.value.clone()) {
                            return Err(Diagnostic::new(
                                DK::Name,
                                param.name.span,
                                format!("duplicate parameter '{}'", param.name.value),
                            ));
                        }
                        if let Some(method) = &method {
                            params.push(*method.signature.params.get(parameter_index).ok_or_else(
                                || {
                                    Diagnostic::new(
                                        DK::Type,
                                        param.name.span,
                                        "method parameter count does not match its trait",
                                    )
                                },
                            )?);
                        } else {
                            params.push(self.catalog.resolve_type(
                                &self.modules,
                                module_index,
                                &param.ty,
                                &type_parameters,
                            )?);
                        }
                    }
                    let ret = if let Some(method) = &method {
                        method.signature.ret
                    } else {
                        function
                            .return_type
                            .as_ref()
                            .map(|ty| {
                                self.catalog.resolve_type(
                                    &self.modules,
                                    module_index,
                                    ty,
                                    &type_parameters,
                                )
                            })
                            .transpose()?
                    };
                    let mut export_symbol = None;
                    let mut external = None;
                    let mut intrinsic = None;
                    for attribute in &function.attributes {
                        match attribute {
                            ast::Attribute::Export { target, symbol }
                            | ast::Attribute::External { target, symbol } => {
                                if target.value != "c" {
                                    return Err(Diagnostic::new(
                                        DK::Unsupported,
                                        target.span,
                                        "only the C boundary target 'c' is supported",
                                    ));
                                }
                                if !valid_export_name(&symbol.value) {
                                    return Err(Diagnostic::new(
                                        DK::Export,
                                        symbol.span,
                                        "C symbol must be a non-reserved ASCII identifier",
                                    ));
                                }
                                if matches!(attribute, ast::Attribute::Export { .. }) {
                                    if !function.public {
                                        return Err(Diagnostic::new(
                                            DK::Export,
                                            function.name.span,
                                            "@export requires a public function",
                                        ));
                                    }
                                    if export_symbol.replace(symbol.value.clone()).is_some() {
                                        return Err(Diagnostic::new(
                                            DK::Export,
                                            symbol.span,
                                            "duplicate @export attribute",
                                        ));
                                    }
                                    if !exports.insert(symbol.value.clone()) {
                                        return Err(Diagnostic::new(
                                            DK::Export,
                                            symbol.span,
                                            format!("duplicate C export symbol '{}'", symbol.value),
                                        ));
                                    }
                                } else if external.replace(symbol.value.clone()).is_some() {
                                    return Err(Diagnostic::new(
                                        DK::Export,
                                        symbol.span,
                                        "duplicate @external attribute",
                                    ));
                                }
                            }
                            ast::Attribute::Intrinsic(span) => {
                                if !module.builtin {
                                    return Err(Diagnostic::new(
                                        DK::Unsupported,
                                        *span,
                                        "@intrinsic is reserved for compiler-owned declarations",
                                    ));
                                }
                                intrinsic = sources::intrinsic(&module.name, &function.name.value);
                                if intrinsic.is_none() {
                                    return Err(Diagnostic::new(
                                        DK::Unsupported,
                                        *span,
                                        "unregistered core intrinsic declaration",
                                    ));
                                }
                            }
                        }
                    }
                    if external.is_some() || intrinsic.is_some() {
                        if function.body.is_some() {
                            return Err(Diagnostic::new(
                                DK::Type,
                                function.name.span,
                                "external and intrinsic functions cannot have a body",
                            ));
                        }
                        if ret.is_none() {
                            return Err(Diagnostic::new(
                                DK::Type,
                                function.name.span,
                                "external and intrinsic functions require a return type",
                            ));
                        }
                    }
                    if !type_parameters.is_empty()
                        && (export_symbol.is_some() || external.is_some())
                    {
                        return Err(Diagnostic::new(
                            DK::Export,
                            function.name.span,
                            "generic functions require a concrete wrapper at the C boundary",
                        ));
                    }
                    let ret = match ret {
                        Some(ret) => Some(ret),
                        None => Some(Ty::Compound(
                            self.solver
                                .try_fresh(
                                    &mut self.catalog.core.types,
                                    super::infer::Constraint::Any,
                                )
                                .map_err(|e| {
                                    Diagnostic::new(DK::Type, function.name.span, e.message)
                                })?,
                        )),
                    };
                    let allowed_errors = if let Some(method) = &method {
                        Some(method.errors_allowed.clone())
                    } else if let Some(errors) = &function.fails {
                        let mut values = Vec::new();
                        for error in errors {
                            let ty = self.catalog.resolve_type(
                                &self.modules,
                                module_index,
                                error,
                                &type_parameters,
                            )?;
                            let ty = data::backend_type(ty);
                            if !matches!(
                                self.catalog.core.ty(ty),
                                crate::contract::ir::Type::Param(_)
                            ) && !matches!(self.catalog.core.ty(ty),crate::contract::ir::Type::Enum(id,_) if self.catalog.core.enums[*id].error)
                            {
                                return Err(Diagnostic::new(
                                    DK::Type,
                                    error.span,
                                    "fails requires an error type",
                                ));
                            }
                            if !values.contains(&ty) {
                                values.push(ty);
                            }
                        }
                        Some(values)
                    } else if function.constant
                        || function.return_type.is_some()
                        || function.body.is_none()
                    {
                        Some(Vec::new())
                    } else {
                        None
                    };
                    let id = DefinitionId(self.definitions.len());
                    if self.module_definitions[module_index]
                        .insert(function.name.value.clone(), id)
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            DK::Name,
                            function.name.span,
                            format!("duplicate function '{}'", function.name.value),
                        ));
                    }
                    let name = if module_index == self.modules.entry {
                        function.name.value.clone()
                    } else {
                        format!("{}.{}", module.name, function.name.value)
                    };
                    self.definitions.push(Definition {
                        module: module_index,
                        ast: function.clone(),
                        signature: Signature {
                            params,
                            ret,
                            sets: InvocationSets {
                                errors: allowed_errors.clone().unwrap_or_default(),
                                ..InvocationSets::default()
                            },
                        },
                        name,
                        exported: export_symbol.is_some(),
                        export_symbol,
                        external,
                        intrinsic,
                        error_mode: super::errors::Mode {
                            allowed: allowed_errors,
                        },
                        bounds,
                        self_type: method.as_ref().and_then(|m| m.self_type),
                        derivation: method.and_then(|m| m.derivation),
                    });
                    Ok(())
                })();
                if let Err(error) = result {
                    errors.push(error.in_source(&module.source_name));
                }
            }
        }
        let mut external_signatures = HashMap::new();
        for definition in &self.definitions {
            if let Some(symbol) = &definition.external {
                if exports.contains(symbol) {
                    errors.push(self.error(
                        definition.module,
                        DK::Export,
                        definition.ast.name.span,
                        format!("external C symbol '{symbol}' collides with an export"),
                    ));
                }
                let signature = (&definition.signature.params, definition.signature.ret);
                if let Some(previous) = external_signatures.insert(symbol, signature) {
                    if previous != signature {
                        errors.push(self.error(
                            definition.module,
                            DK::Type,
                            definition.ast.name.span,
                            format!("external C symbol '{symbol}' has conflicting signatures"),
                        ));
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
    fn imports(&mut self) -> Result<(), Vec<Diagnostic>> {
        let module_names: HashMap<_, _> = self
            .modules
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.clone(), i))
            .collect();
        let mut errors = Vec::new();
        for (index, module) in self.modules.modules.iter().enumerate() {
            for import in &module.ast.imports {
                let Some(&target) = module_names.get(&import.path.value) else {
                    continue;
                };
                if let Some(members) = &import.members {
                    for member in members {
                        let symbol = if let Some(id) =
                            self.module_definitions[target].get(&member.value)
                        {
                            if !self.definitions[id.0].ast.public {
                                errors.push(
                                    Diagnostic::new(
                                        DK::Name,
                                        member.span,
                                        format!(
                                            "'{}.{}' is private",
                                            import.path.value, member.value
                                        ),
                                    )
                                    .in_source(&module.source_name),
                                );
                                continue;
                            }
                            Some(Symbol::Function(*id))
                        } else if let Some(constant) = constant(&import.path.value, &member.value) {
                            Some(Symbol::Constant(constant))
                        } else if self.traits.find_trait(index, &member.value).is_some() {
                            continue;
                        } else if let Some(id) = self.catalog.type_id(target, &member.value) {
                            if !self.catalog.declaration(id).ast.public {
                                errors.push(self.error(
                                    index,
                                    DK::Name,
                                    member.span,
                                    "imported type is private",
                                ));
                            }
                            continue;
                        } else {
                            match self.catalog.constructor(
                                &self.modules,
                                index,
                                &member.value,
                                member.span,
                            ) {
                                Ok(value) => value.map(Symbol::Constructor),
                                Err(error) => {
                                    errors.push(error.in_source(&module.source_name));
                                    continue;
                                }
                            }
                        };
                        match symbol {
                            Some(symbol) => {
                                if self.module_definitions[index].contains_key(&member.value)
                                    || self.scopes[index]
                                        .values
                                        .insert(member.value.clone(), symbol)
                                        .is_some_and(|previous| previous != symbol)
                                {
                                    errors.push(Diagnostic::new(DK::Name, member.span, format!("imported name '{}' conflicts with another declaration", member.value)).in_source(&module.source_name));
                                }
                            }
                            None => errors.push(
                                Diagnostic::new(
                                    DK::Name,
                                    member.span,
                                    format!(
                                        "module '{}' has no public declaration '{}'",
                                        import.path.value, member.value
                                    ),
                                )
                                .in_source(&module.source_name),
                            ),
                        }
                    }
                }
                // A selective import also introduces the module's short name.
                // Repeating the same import does not introduce a second definition.
                let alias = import.path.value.rsplit('.').next().unwrap().to_owned();
                if self.scopes[index]
                    .modules
                    .insert(alias.clone(), target)
                    .is_some_and(|previous| previous != target)
                {
                    errors.push(
                        Diagnostic::new(
                            DK::Name,
                            import.path.span,
                            format!("duplicate imported module '{alias}'"),
                        )
                        .in_source(&module.source_name),
                    );
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    fn function(&mut self, id: DefinitionId) -> Result<Ty, Diagnostic> {
        let definition = self.definitions[id.0].clone();
        if self.states[id.0] != 0 {
            self.schemes.reference(id);
            return definition.signature.ret.ok_or_else(|| {
                self.error(
                    definition.module,
                    DK::Type,
                    definition.ast.name.span,
                    "recursive functions require an explicit return type",
                )
            });
        }
        self.schemes.begin(id);
        let result = self.function_body(id);
        let component = self.schemes.finish(id);
        let ret = result?;
        if !component.is_empty() {
            self.constrain_errors().map_err(|errors| {
                errors
                    .into_iter()
                    .next()
                    .expect("failure inference reports a diagnostic")
            })?;
            let signatures = self
                .definitions
                .iter()
                .map(|d| d.signature.clone())
                .collect::<Vec<_>>();
            let constants = self
                .definitions
                .iter()
                .map(|d| d.ast.constant)
                .collect::<Vec<_>>();
            self.schemes
                .generalize(
                    &component,
                    &signatures,
                    &constants,
                    &mut self.catalog.core.types,
                    &mut self.solver,
                )
                .map_err(|e| {
                    self.error(
                        definition.module,
                        DK::Type,
                        definition.ast.name.span,
                        e.message,
                    )
                })?;
            for member in component {
                if let Some(function) = &mut self.checked[member.0] {
                    function.type_parameters = self.schemes.parameters[member.0].len();
                    if function.type_parameters != 0 && function.exported {
                        return Err(Diagnostic::new(
                            DK::Export,
                            function.span,
                            "generic functions require a concrete wrapper at the C boundary",
                        )
                        .in_source(&function.source_name));
                    }
                }
            }
        }
        Ok(ret)
    }
    fn constrain_errors(&mut self) -> Result<(), Vec<Diagnostic>> {
        let modes = self
            .definitions
            .iter()
            .enumerate()
            .map(|(index, definition)| (DefinitionId(index), definition.error_mode.clone()))
            .collect();
        let mut program = ir::Program {
            core: self.catalog.core.clone(),
            registry: self.traits.registry.clone(),
            functions: self.checked.iter().flatten().cloned().collect(),
        };
        super::errors::constrain(
            &mut program,
            &modes,
            &mut self.catalog,
            &mut self.solver,
            &self.schemes.parameters,
        )?;
        for function in program.functions {
            self.definitions[function.id.0].signature.sets = function.signature.sets.clone();
            if let Some(checked) = &mut self.checked[function.id.0] {
                checked.signature.sets = function.signature.sets;
            }
        }
        Ok(())
    }
    fn function_body(&mut self, id: DefinitionId) -> Result<Ty, Diagnostic> {
        let definition = self.definitions[id.0].clone();
        self.states[id.0] = 1;
        let mut locals = Locals {
            names: HashMap::new(),
            next: 0,
        };
        let params: Vec<_> = definition
            .ast
            .params
            .iter()
            .zip(&definition.signature.params)
            .map(|(p, ty)| locals.bind(p.name.value.clone(), *ty))
            .collect();
        let saved_parameters = std::mem::replace(
            &mut self.current_parameters,
            definition
                .ast
                .type_params
                .iter()
                .enumerate()
                .map(|(i, p)| (p.name.value.clone(), i))
                .collect(),
        );
        let saved_bounds = std::mem::replace(&mut self.current_bounds, definition.bounds.clone());
        let saved_self = std::mem::replace(&mut self.current_self_type, definition.self_type);
        let body = definition
            .ast
            .body
            .as_ref()
            .map(|body| {
                self.block(
                    definition.module,
                    body,
                    &mut locals,
                    definition.signature.ret,
                )
            })
            .transpose();
        self.current_parameters = saved_parameters;
        self.current_bounds = saved_bounds;
        self.current_self_type = saved_self;
        let body = body?;
        if definition.ast.return_type.is_none()
            && body.as_ref().is_some_and(|body| body.ty == Ty::Never)
        {
            if let Some(ret) = definition.signature.ret {
                let resolved = self
                    .solver
                    .resolve(&mut self.catalog.core.types, data::backend_type(ret))
                    .map_err(|e| {
                        self.error(
                            definition.module,
                            DK::Type,
                            definition.ast.name.span,
                            e.message,
                        )
                    })?;
                if self.solver.is_meta(resolved) {
                    self.solver
                        .unify(
                            &mut self.catalog.core.types,
                            data::backend_type(ret),
                            crate::contract::ir::TypeId::NEVER,
                        )
                        .map_err(|e| {
                            self.error(
                                definition.module,
                                DK::Type,
                                definition.ast.name.span,
                                e.message,
                            )
                        })?;
                }
            }
        }
        let ret = definition
            .signature
            .ret
            .or_else(|| body.as_ref().map(|b| b.ty))
            .unwrap_or(Ty::Unit);
        self.definitions[id.0].signature.ret = Some(ret);
        self.checked[id.0] = Some(ir::Function {
            id,
            type_parameters: definition.ast.type_params.len(),
            constant: definition.ast.constant,
            origin: definition.ast.constant.then_some(id),
            derivation: definition.derivation,
            name: definition.name,
            source_name: self.modules.modules[definition.module].source_name.clone(),
            span: definition.ast.name.span,
            exported: definition.exported,
            export_symbol: definition.export_symbol,
            signature: Signature {
                ret: Some(ret),
                ..definition.signature
            },
            params,
            external: definition.external,
            intrinsic: definition.intrinsic,
            body,
        });
        self.states[id.0] = 2;
        Ok(ret)
    }
    fn block(
        &mut self,
        module: usize,
        block: &ast::Block,
        locals: &mut Locals,
        expected: Option<Ty>,
    ) -> Result<ir::Block, Diagnostic> {
        let saved = locals.names.clone();
        let mut statements = Vec::new();
        let mut ty = Ty::Unit;
        let mut terminated = false;
        for (index, statement) in block.statements.iter().enumerate() {
            let last = index + 1 == block.statements.len();
            match statement {
                ast::Statement::Bind {
                    pattern,
                    annotation,
                    value,
                } => {
                    let annotation = annotation
                        .as_ref()
                        .map(|ty| {
                            self.catalog.resolve_type_with_self(
                                &self.modules,
                                module,
                                ty,
                                &self.current_parameters,
                                self.current_self_type.map(data::frontend_type),
                            )
                        })
                        .transpose()
                        .map_err(|e| e.in_source(&self.modules.modules[module].source_name))?;
                    let value = self.expr(module, value, locals, annotation)?;
                    let pattern =
                        self.pattern(module, pattern, value.ty, locals, &mut HashSet::new())?;
                    if !super::patterns::exhaustive(
                        std::slice::from_ref(&pattern),
                        value.ty,
                        &mut self.catalog,
                    ) {
                        return Err(self.error(
                            module,
                            DK::Type,
                            pattern.span,
                            "binding pattern can fail; use postfix match",
                        ));
                    }
                    if value.ty == Ty::Never {
                        terminated = true;
                    }
                    statements.push(ir::Statement::Pattern { pattern, value });
                    ty = Ty::Unit;
                }
                ast::Statement::Expr(expression) => {
                    let expression = self.expr(
                        module,
                        expression,
                        locals,
                        if last { expected } else { None },
                    )?;
                    ty = expression.ty;
                    if ty == Ty::Never {
                        terminated = true;
                    }
                    statements.push(ir::Statement::Expr(expression));
                }
            }
        }
        if terminated {
            ty = Ty::Never;
        }
        if let Some(expected) = expected {
            self.expect_type(module, ty, expected, block.span)?;
        }
        locals.names = saved;
        Ok(ir::Block {
            statements,
            ty,
            span: block.span,
        })
    }
    fn expr(
        &mut self,
        module: usize,
        expression: &ast::Expr,
        locals: &mut Locals,
        expected: Option<Ty>,
    ) -> Result<ir::Expr, Diagnostic> {
        use ast::ExprKind as A;
        let span = expression.span;
        let expected = expected
            .map(|ty| self.resolve_type(module, ty, span))
            .transpose()?;
        let (kind, ty) = match &expression.kind {
            A::Integer(raw) => {
                let target = expected.filter(|t| *t == Ty::Int64).unwrap_or(Ty::Int);
                let clean = raw.replace('_', "");
                let (digits, radix) = if let Some(s) = clean.strip_prefix("0x") {
                    (s, 16)
                } else if let Some(s) = clean.strip_prefix("0b") {
                    (s, 2)
                } else {
                    (clean.as_str(), 10)
                };
                let number = u64::from_str_radix(digits, radix).ok().filter(|n| *n <= if target == Ty::Int64 { i64::MAX as u64 } else { i32::MAX as u64 }).ok_or_else(|| self.error(module, DK::Range, span, format!("integer literal is outside the nonnegative {target} range; use {target}.MIN for the minimum value")))?;
                (
                    ir::ExprKind::Literal(if target == Ty::Int64 {
                        ir::Literal::Int64(number as i64)
                    } else {
                        ir::Literal::Int(number as i32)
                    }),
                    target,
                )
            }
            A::Float(raw) => {
                let number = raw
                    .replace('_', "")
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| {
                        self.error(
                            module,
                            DK::Range,
                            span,
                            "Float literal is outside the finite binary64 range",
                        )
                    })?;
                (ir::ExprKind::Literal(ir::Literal::Float(number)), Ty::Float)
            }
            A::Bool(value) => (ir::ExprKind::Literal(ir::Literal::Bool(*value)), Ty::Bool),
            A::Unit => (ir::ExprKind::Literal(ir::Literal::Unit), Ty::Unit),
            A::String(value) => (
                ir::ExprKind::Literal(ir::Literal::String(value.clone())),
                Ty::String,
            ),
            A::Interpolate(parts) => {
                let mut values = Vec::new();
                for part in parts {
                    let value = self.expr(module, part, locals, None)?;
                    if value.ty == Ty::String {
                        values.push(value);
                    } else {
                        values.push(self.show(module, value, false, part.span)?);
                    }
                }
                (ir::ExprKind::Concat(values), Ty::String)
            }
            A::Match { value, arms } => {
                let mut value = self.expr(module, value, locals, None)?;
                value.ty = self.resolve_type(module, value.ty, span)?;
                let mut checked = Vec::new();
                let mut coverage = Vec::new();
                let mut result = expected;
                for arm in arms {
                    let saved = locals.names.clone();
                    let pattern =
                        self.pattern(module, &arm.pattern, value.ty, locals, &mut HashSet::new())?;
                    let guard = arm
                        .guard
                        .as_ref()
                        .map(|guard| self.expr(module, guard, locals, Some(Ty::Bool)))
                        .transpose()?;
                    if guard.is_none() {
                        coverage.push(pattern.clone());
                    }
                    let body = self.expr(module, &arm.value, locals, result)?;
                    if result.is_none() && body.ty != Ty::Never {
                        result = Some(body.ty);
                    }
                    checked.push(ir::Arm {
                        pattern,
                        guard,
                        value: body,
                    });
                    locals.names = saved;
                }
                if !super::patterns::exhaustive(&coverage, value.ty, &mut self.catalog) {
                    return Err(self.error(module, DK::Type, span, "match is not exhaustive"));
                }
                (
                    ir::ExprKind::Match {
                        value: Box::new(value),
                        arms: checked,
                    },
                    result.unwrap_or(Ty::Never),
                )
            }
            A::Fail(value) => {
                let value = self.expr(module, value, locals, None)?;
                if value.ty != Ty::Any
                    && !matches!(
                        self.catalog.core.ty(data::backend_type(value.ty)),
                        crate::contract::ir::Type::Param(_)
                    )
                    && !matches!(self.catalog.core.ty(data::backend_type(value.ty)),crate::contract::ir::Type::Enum(id,_) if self.catalog.core.enums[*id].error)
                {
                    return Err(self.error(
                        module,
                        DK::Type,
                        value.span,
                        "fail requires an error value",
                    ));
                }
                (ir::ExprKind::Fail(Box::new(value)), Ty::Never)
            }
            A::Escape { value, arms } => {
                let value = self.expr(module, value, locals, expected)?;
                let result = if value.ty == Ty::Never {
                    expected.unwrap_or(Ty::Compound(
                        self.solver
                            .try_fresh(&mut self.catalog.core.types, super::infer::Constraint::Any)
                            .map_err(|e| self.error(module, DK::Type, span, e.message))?,
                    ))
                } else {
                    value.ty
                };
                let error = Local {
                    id: LocalId(locals.next),
                    name: "error".into(),
                    ty: Ty::Any,
                };
                locals.next += 1;
                let known = self.known_errors(module, &value)?;
                let mut checked = Vec::new();
                for arm in arms {
                    let saved = locals.names.clone();
                    let pattern_type = if let ast::PatternKind::Constructor { path, .. } =
                        &arm.pattern.kind
                    {
                        match self.resolve(module, path)? {
                            Symbol::Constructor(ctor) => {
                                if !self.catalog.core.enums[ctor.enum_id].error {
                                    return Err(self.error(
                                        module,
                                        DK::Type,
                                        arm.pattern.span,
                                        "escape constructor is not an error",
                                    ));
                                }
                                if let Some(&ty)=known.iter().find(|&&ty|matches!(self.catalog.core.ty(ty),crate::contract::ir::Type::Enum(id,_) if *id==ctor.enum_id)) {data::frontend_type(ty)}
                        else if self.catalog.core.enums[ctor.enum_id].params==0 {Ty::Compound(self.catalog.core.types.enum_type(ctor.enum_id,vec![]))}
                        else {
                            let arguments=(0..self.catalog.core.enums[ctor.enum_id].params).map(|_|self.solver.try_fresh(&mut self.catalog.core.types,super::infer::Constraint::Any)).collect::<Result<Vec<_>,_>>()
                                .map_err(|e|self.error(module,DK::Type,arm.pattern.span,e.message))?;
                            Ty::Compound(self.catalog.core.types.enum_type(ctor.enum_id,arguments))
                        }
                            }
                            _ => {
                                return Err(self.error(
                                    module,
                                    DK::Type,
                                    arm.pattern.span,
                                    "escape requires error patterns",
                                ));
                            }
                        }
                    } else {
                        return Err(self.error(module,DK::Type,arm.pattern.span,"escape arms require error constructors; wildcards and bindings are not allowed here"));
                    };
                    let pattern = self.pattern(
                        module,
                        &arm.pattern,
                        pattern_type,
                        locals,
                        &mut HashSet::new(),
                    )?;
                    let guard = arm
                        .guard
                        .as_ref()
                        .map(|g| self.expr(module, g, locals, Some(Ty::Bool)))
                        .transpose()?;
                    let body = self.expr(module, &arm.value, locals, Some(result))?;
                    checked.push(ir::Arm {
                        pattern,
                        guard,
                        value: body,
                    });
                    locals.names = saved;
                }
                (
                    ir::ExprKind::Escape {
                        value: Box::new(value),
                        error,
                        arms: checked,
                    },
                    result,
                )
            }
            A::Tuple(elements) => {
                let hints =
                    expected.and_then(|ty| match self.catalog.core.ty(data::backend_type(ty)) {
                        crate::contract::ir::Type::Tuple(fields) => Some(
                            fields
                                .iter()
                                .map(|&t| data::frontend_type(t))
                                .collect::<Vec<_>>(),
                        ),
                        _ => None,
                    });
                if hints.as_ref().is_some_and(|h| h.len() != elements.len()) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "tuple arity does not match its expected type",
                    ));
                }
                let mut checked = Vec::new();
                for (index, element) in elements.iter().enumerate() {
                    checked.push(self.expr(
                        module,
                        element,
                        locals,
                        hints.as_ref().map(|h| h[index]),
                    )?);
                }
                let ty = Ty::Compound(self.catalog.core.types.intern(
                    crate::contract::ir::Type::Tuple(
                        checked.iter().map(|e| data::backend_type(e.ty)).collect(),
                    ),
                ));
                (ir::ExprKind::Tuple(checked), ty)
            }
            A::Field { base, name } => {
                let mut base = self.expr(module, base, locals, None)?;
                base.ty = self.resolve_type(module, base.ty, span)?;
                let crate::contract::ir::Type::Enum(id, _) =
                    self.catalog.core.ty(data::backend_type(base.ty))
                else {
                    return Err(self.error(
                        module,
                        DK::Type,
                        name.span,
                        "field access requires a record value",
                    ));
                };
                let id = *id;
                let declaration = self.catalog.declaration(id);
                if declaration.ast.opaque && declaration.module != module {
                    return Err(self.error(
                        module,
                        DK::Name,
                        name.span,
                        "opaque fields are private",
                    ));
                }
                if declaration.ast.variants.len() != 1 {
                    return Err(self.error(
                        module,
                        DK::Type,
                        name.span,
                        "enum fields must be read through a matching pattern",
                    ));
                }
                let index = declaration.ast.variants[0]
                    .fields
                    .iter()
                    .position(|f| f.name.as_ref().is_some_and(|n| n.value == name.value))
                    .ok_or_else(|| {
                        self.error(
                            module,
                            DK::Name,
                            name.span,
                            format!("unknown field '{}'", name.value),
                        )
                    })?;
                let ctor = crate::contract::ir::CtorRef {
                    enum_id: id,
                    variant: 0,
                };
                let ty = self.catalog.fields(ctor, base.ty)[index];
                (
                    ir::ExprKind::Field {
                        base: Box::new(base),
                        ctor,
                        index,
                    },
                    ty,
                )
            }
            A::Name(path) => {
                if let Some(local) = (path.len() == 1)
                    .then(|| locals.names.get(&path[0].value))
                    .flatten()
                {
                    (ir::ExprKind::Local(local.id), local.ty)
                } else {
                    match self.resolve(module, path)? {
                        Symbol::Constant(id) => (ir::ExprKind::Literal(id.literal()), id.ty()),
                        Symbol::Constructor(ctor) => {
                            return self.construct(module, ctor, &[], locals, expected, span);
                        }
                        Symbol::Function(id) if self.definitions[id.0].ast.constant => {
                            self.function(id)?;
                            let ret = self.definitions[id.0]
                                .signature
                                .ret
                                .expect("const inference return type");
                            if let Some(expected) = expected {
                                self.expect_type(module, ret, expected, span)?;
                            }
                            let ret = self.resolve_type(module, ret, span)?;
                            return Ok(ir::Expr {
                                kind: ir::ExprKind::Call {
                                    target: id,
                                    type_arguments: vec![],
                                    args: vec![],
                                    order: vec![],
                                },
                                ty: ret,
                                span,
                            });
                        }
                        Symbol::Function(_) => {
                            return Err(self.error(
                                module,
                                DK::Unsupported,
                                span,
                                "function values require P4; call this function directly",
                            ));
                        }
                    }
                }
            }
            A::Unary { op, value } => {
                let value = self.expr(
                    module,
                    value,
                    locals,
                    if *op == ast::Unary::Not {
                        Some(Ty::Bool)
                    } else {
                        expected.filter(|t| t.numeric())
                    },
                )?;
                if *op == ast::Unary::Neg {
                    self.numeric(module, value.ty, span)?;
                }
                let ty = value.ty;
                (
                    ir::ExprKind::Unary {
                        op: *op,
                        value: Box::new(value),
                    },
                    ty,
                )
            }
            A::Binary {
                op: ast::Binary::Pipe,
                left,
                right,
            } => {
                let (callee, mut args) = match &right.kind {
                    A::Call { callee, args } => (callee.as_ref().clone(), args.clone()),
                    _ => (right.as_ref().clone(), Vec::new()),
                };
                args.insert(
                    0,
                    ast::Argument {
                        label: None,
                        value: left.as_ref().clone(),
                    },
                );
                return self.expr(
                    module,
                    &ast::Expr {
                        kind: A::Call {
                            callee: Box::new(callee),
                            args,
                        },
                        span,
                    },
                    locals,
                    expected,
                );
            }
            A::Binary { op, left, right } => {
                let logical = matches!(op, ast::Binary::And | ast::Binary::Or);
                let hint = if logical {
                    Some(Ty::Bool)
                } else {
                    expected
                        .filter(|t| t.numeric() && !op.comparison())
                        .or_else(|| self.hint(module, left, locals))
                        .or_else(|| self.hint(module, right, locals))
                };
                let left = self.expr(module, left, locals, hint)?;
                let right = self.expr(
                    module,
                    right,
                    locals,
                    if left.ty == Ty::Never {
                        hint
                    } else {
                        Some(left.ty)
                    },
                )?;
                if op.comparison() {
                    return self.comparison(module, *op, left, right, span);
                }
                let left_ty = self.resolve_type(module, left.ty, span)?;
                if !logical {
                    self.numeric(module, left_ty, span)?;
                }
                let ty = if logical || op.comparison() {
                    Ty::Bool
                } else if left.ty == Ty::Never {
                    right.ty
                } else {
                    left.ty
                };
                (
                    ir::ExprKind::Binary {
                        op: *op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty,
                )
            }
            A::Call { callee, args } => {
                if let A::Field { base, name } = &callee.kind {
                    return self.method_call(module, base, name, args, locals, expected, span);
                }
                let A::Name(path) = &callee.kind else {
                    return Err(self.error(
                        module,
                        DK::Unsupported,
                        callee.span,
                        "indirect calls require P4",
                    ));
                };
                if path.len() >= 2 {
                    let prefix = path[..path.len() - 1]
                        .iter()
                        .map(|s| s.value.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    if let Some(trait_id) = self.traits.find_trait(module, &prefix) {
                        return self.source_trait_call(
                            module,
                            trait_id,
                            &path.last().unwrap().value,
                            args,
                            locals,
                            expected,
                            span,
                        );
                    }
                }
                if path.len() == 1 && locals.names.contains_key(&path[0].value) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        callee.span,
                        format!(
                            "'{}' is a scalar binding and cannot be called",
                            path[0].value
                        ),
                    ));
                }
                let symbol = match self.resolve(module, path) {
                    Ok(symbol) => symbol,
                    Err(error) => {
                        if path.len() > 1 {
                            let prefix = &path[..path.len() - 1];
                            let value = match self.resolve(module, prefix) {
                                Ok(Symbol::Constructor(ctor)) => {
                                    self.catalog.core.enums[ctor.enum_id].variants[ctor.variant]
                                        .tys
                                        .is_empty()
                                }
                                Ok(Symbol::Function(id)) => self.definitions[id.0].ast.constant,
                                Ok(Symbol::Constant(_)) => true,
                                Err(_) => false,
                            };
                            if value {
                                let base = ast::Expr {
                                    kind: A::Name(prefix.to_vec()),
                                    span: Span::new(
                                        callee.span.start,
                                        prefix.last().unwrap().span.end,
                                    ),
                                };
                                return self.method_call(
                                    module,
                                    &base,
                                    path.last().unwrap(),
                                    args,
                                    locals,
                                    expected,
                                    span,
                                );
                            }
                        }
                        return Err(error);
                    }
                };
                let id = match symbol {
                    Symbol::Function(id) => id,
                    Symbol::Constructor(ctor) => {
                        return self.construct(module, ctor, args, locals, expected, span);
                    }
                    Symbol::Constant(_) => {
                        return Err(self.error(
                            module,
                            DK::Type,
                            callee.span,
                            "a numeric constant cannot be called",
                        ));
                    }
                };
                if self.definitions[id.0].ast.constant {
                    return Err(self.error(
                        module,
                        DK::Type,
                        callee.span,
                        "a const value cannot be called",
                    ));
                }
                self.function(id)?;
                let mut signature = self.definitions[id.0].signature.clone();
                let type_arguments = (0..self.schemes.parameters[id.0].len())
                    .map(|_| {
                        self.solver
                            .try_fresh(&mut self.catalog.core.types, super::infer::Constraint::Any)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| self.error(module, DK::Type, span, e.message))?;
                let substitutions: HashMap<_, _> = self.schemes.parameters[id.0]
                    .iter()
                    .copied()
                    .zip(type_arguments.iter().copied())
                    .collect();
                if !type_arguments.is_empty() {
                    if let Some(ret) = signature.ret {
                        let resolved = self
                            .solver
                            .resolve(&mut self.catalog.core.types, data::backend_type(ret))
                            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
                        if self.solver.is_meta(resolved) {
                            let instance = self
                                .solver
                                .try_fresh(
                                    &mut self.catalog.core.types,
                                    super::infer::Constraint::Any,
                                )
                                .map_err(|e| self.error(module, DK::Type, span, e.message))?;
                            self.schemes.defer_return(
                                id,
                                resolved,
                                instance,
                                substitutions.clone(),
                            );
                            signature.ret = Some(data::frontend_type(instance));
                        }
                    }
                }
                for parameter in signature.params.iter_mut().chain(signature.ret.iter_mut()) {
                    *parameter = data::frontend_type(
                        self.solver
                            .substitute(
                                &mut self.catalog.core.types,
                                data::backend_type(*parameter),
                                &substitutions,
                            )
                            .map_err(|e| self.error(module, DK::Type, span, e.message))?,
                    );
                }
                if let (Some(ret), Some(expected)) = (signature.ret, expected) {
                    self.expect_type(module, ret, expected, span)?;
                }

                let params = self.definitions[id.0].ast.params.clone();
                let mut order = vec![usize::MAX; params.len()];
                let mut named = false;
                let mut checked = Vec::new();
                for (index, argument) in args.iter().enumerate() {
                    let parameter = if let Some(label) = &argument.label {
                        named = true;
                        params
                            .iter()
                            .position(|p| p.name.value == label.value)
                            .ok_or_else(|| {
                                self.error(
                                    module,
                                    DK::Type,
                                    label.span,
                                    format!("unknown argument label '{}'", label.value),
                                )
                            })?
                    } else {
                        if named {
                            return Err(self.error(
                                module,
                                DK::Type,
                                argument.value.span,
                                "positional arguments must precede named arguments",
                            ));
                        }
                        if index >= params.len() {
                            return Err(self.error(
                                module,
                                DK::Type,
                                span,
                                format!(
                                    "function '{}' expects {} arguments, found {}",
                                    self.definitions[id.0].name,
                                    params.len(),
                                    args.len()
                                ),
                            ));
                        }
                        index
                    };
                    if order[parameter] != usize::MAX {
                        return Err(self.error(
                            module,
                            DK::Type,
                            argument.value.span,
                            format!(
                                "argument '{}' is supplied more than once",
                                params[parameter].name.value
                            ),
                        ));
                    }
                    order[parameter] = index;
                    checked.push(self.expr(
                        module,
                        &argument.value,
                        locals,
                        Some(signature.params[parameter]),
                    )?);
                }
                if order.contains(&usize::MAX) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        format!(
                            "function '{}' expects {} arguments, found {}",
                            self.definitions[id.0].name,
                            params.len(),
                            args.len()
                        ),
                    ));
                }
                self.validate_call_bounds(module, id, &type_arguments, span)?;
                let ret = match signature.ret {
                    Some(ret) => ret,
                    None => self.function(id)?,
                };
                if let Some(intrinsic) = self.definitions[id.0].intrinsic {
                    let op = match intrinsic {
                        Intrinsic::Panic => Some(crate::contract::ir::Builtin::Panic),
                        Intrinsic::Todo => Some(crate::contract::ir::Builtin::Todo),
                        Intrinsic::Dbg => Some(crate::contract::ir::Builtin::Dbg),
                        _ => None,
                    };
                    if let Some(op) = op {
                        return Ok(ir::Expr {
                            kind: ir::ExprKind::Builtin { op, args: checked },
                            ty: self.resolve_type(module, ret, span)?,
                            span,
                        });
                    }
                    if matches!(intrinsic, Intrinsic::Inspect | Intrinsic::ShowNested) {
                        return self.show(module, checked.remove(0), true, span);
                    }
                }
                (
                    ir::ExprKind::Call {
                        target: id,
                        type_arguments,
                        args: checked,
                        order,
                    },
                    ret,
                )
            }
            A::If {
                condition,
                then,
                else_,
            } => {
                let condition = self.expr(module, condition, locals, Some(Ty::Bool))?;
                let then = self.block(
                    module,
                    then,
                    locals,
                    if else_.is_none() {
                        Some(Ty::Unit)
                    } else {
                        expected
                    },
                )?;
                let else_ = match else_ {
                    Some(block) => self.block(
                        module,
                        block,
                        locals,
                        expected.or_else(|| (then.ty != Ty::Never).then_some(then.ty)),
                    )?,
                    None => ir::Block {
                        statements: Vec::new(),
                        ty: Ty::Unit,
                        span,
                    },
                };
                let ty = if then.ty == Ty::Never {
                    else_.ty
                } else {
                    then.ty
                };
                self.expect_type(module, else_.ty, ty, span)?;
                (
                    ir::ExprKind::If {
                        condition: Box::new(condition),
                        then,
                        else_,
                    },
                    ty,
                )
            }
            A::Block(block) => {
                let block = self.block(module, block, locals, expected)?;
                let ty = block.ty;
                (ir::ExprKind::Block(block), ty)
            }
            A::Assert(condition) => {
                let condition = self.expr(module, condition, locals, Some(Ty::Bool))?;
                (ir::ExprKind::Assert(Box::new(condition)), Ty::Unit)
            }
        };
        if let Some(expected) = expected {
            self.expect_type(module, ty, expected, span)?;
        }
        let ty = self.resolve_type(module, ty, span)?;
        self.validate_type_bounds(module, data::backend_type(ty), &self.current_bounds, span)?;
        Ok(ir::Expr { kind, ty, span })
    }
    fn known_errors(
        &mut self,
        module: usize,
        value: &ir::Expr,
    ) -> Result<Vec<crate::contract::ir::TypeId>, Diagnostic> {
        let functions = self.checked.iter().flatten().cloned().collect::<Vec<_>>();
        super::errors::incoming(
            value,
            &functions,
            &self.traits.registry,
            &mut self.catalog,
            &mut self.solver,
            &self.schemes.parameters,
            &self.modules.modules[module].source_name,
        )
    }

    fn resolve_type(&mut self, module: usize, ty: Ty, span: Span) -> Result<Ty, Diagnostic> {
        self.solver
            .resolve(&mut self.catalog.core.types, data::backend_type(ty))
            .map(data::frontend_type)
            .map_err(|e| self.error(module, DK::Type, span, e.message))
    }
    fn expect_type(
        &mut self,
        module: usize,
        actual: Ty,
        expected: Ty,
        span: Span,
    ) -> Result<(), Diagnostic> {
        self.solver
            .unify(
                &mut self.catalog.core.types,
                data::backend_type(actual),
                data::backend_type(expected),
            )
            .map_err(|e| self.error(module, DK::Type, span, e.message))
    }
    fn numeric(&mut self, module: usize, ty: Ty, span: Span) -> Result<(), Diagnostic> {
        let numeric = self
            .solver
            .try_fresh(
                &mut self.catalog.core.types,
                super::infer::Constraint::Numeric,
            )
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        self.solver
            .unify(
                &mut self.catalog.core.types,
                data::backend_type(ty),
                numeric,
            )
            .map_err(|e| self.error(module, DK::Type, span, e.message))
    }

    fn pattern(
        &mut self,
        module: usize,
        pattern: &ast::Pattern,
        value_type: Ty,
        locals: &mut Locals,
        names: &mut HashSet<String>,
    ) -> Result<ir::Pattern, Diagnostic> {
        use crate::contract::ir::Type;
        use ast::PatternKind as P;
        let span = pattern.span;
        let kind = match &pattern.kind {
            P::Any => ir::PatternKind::Any,
            P::Bind(name) => {
                if !names.insert(name.value.clone()) {
                    return Err(self.error(
                        module,
                        DK::Name,
                        name.span,
                        format!("duplicate pattern binding '{}'", name.value),
                    ));
                }
                ir::PatternKind::Bind(locals.bind(name.value.clone(), value_type))
            }
            P::Literal(expression) => {
                let literal = if let ast::ExprKind::Unary {
                    op: ast::Unary::Neg,
                    value,
                } = &expression.kind
                {
                    if let ast::ExprKind::Integer(raw) = &value.kind {
                        let text = raw.replace('_', "");
                        let (text, radix) = if let Some(s) = text.strip_prefix("0x") {
                            (s, 16)
                        } else if let Some(s) = text.strip_prefix("0b") {
                            (s, 2)
                        } else {
                            (text.as_str(), 10)
                        };
                        let limit = if value_type == Ty::Int64 {
                            (i64::MAX as u64) + 1
                        } else {
                            (i32::MAX as u64) + 1
                        };
                        let number = u64::from_str_radix(text, radix)
                            .ok()
                            .filter(|n| *n <= limit)
                            .ok_or_else(|| {
                                self.error(
                                    module,
                                    DK::Range,
                                    span,
                                    "negative pattern literal is outside its value type range",
                                )
                            })?;
                        match value_type {
                            Ty::Int => ir::Literal::Int((-(number as i128)) as i32),
                            Ty::Int64 => ir::Literal::Int64((-(number as i128)) as i64),
                            _ => {
                                return Err(self.error(
                                    module,
                                    DK::Type,
                                    span,
                                    "integer pattern requires Int or Int64",
                                ));
                            }
                        }
                    } else if let ast::ExprKind::Float(raw) = &value.kind {
                        if value_type != Ty::Float {
                            return Err(self.error(
                                module,
                                DK::Type,
                                span,
                                "Float pattern has the wrong value type",
                            ));
                        }
                        let number = raw
                            .replace('_', "")
                            .parse::<f64>()
                            .ok()
                            .filter(|n| n.is_finite())
                            .ok_or_else(|| {
                                self.error(
                                    module,
                                    DK::Range,
                                    span,
                                    "Float pattern is outside the finite range",
                                )
                            })?;
                        ir::Literal::Float(-number)
                    } else {
                        return Err(self.error(
                            module,
                            DK::Syntax,
                            span,
                            "negative patterns require a numeric literal",
                        ));
                    }
                } else {
                    let value = self.expr(module, expression, locals, Some(value_type))?;
                    let ir::ExprKind::Literal(literal) = value.kind else {
                        return Err(self.error(
                            module,
                            DK::Syntax,
                            span,
                            "pattern requires a constant literal",
                        ));
                    };
                    literal
                };
                ir::PatternKind::Literal(literal)
            }
            P::Tuple(fields) => {
                let Type::Tuple(types) = self.catalog.core.ty(data::backend_type(value_type))
                else {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "tuple pattern requires a tuple value",
                    ));
                };
                let types = types.clone();
                if types.len() != fields.len() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "tuple pattern has the wrong arity",
                    ));
                }
                let mut checked = Vec::new();
                for (field, ty) in fields.iter().zip(types) {
                    checked.push(self.pattern(
                        module,
                        field,
                        data::frontend_type(ty),
                        locals,
                        names,
                    )?);
                }
                ir::PatternKind::Tuple(checked)
            }
            P::Constructor { path, args, rest } => {
                if let Symbol::Function(id) = self.resolve(module, path)? {
                    if self.definitions[id.0].ast.constant {
                        if !args.is_empty() || *rest {
                            return Err(self.error(
                                module,
                                DK::Syntax,
                                span,
                                "a const pattern cannot take constructor arguments",
                            ));
                        }
                        let value = self.expr(
                            module,
                            &ast::Expr {
                                kind: ast::ExprKind::Name(path.clone()),
                                span,
                            },
                            locals,
                            Some(value_type),
                        )?;
                        let coverage =
                            super::constant_patterns::coverage(&value, &self.checked).map(Box::new);
                        return Ok(ir::Pattern {
                            kind: ir::PatternKind::Constant {
                                value: Box::new(value),
                                coverage,
                                comparison: None,
                            },
                            ty: value_type,
                            span,
                        });
                    }
                }
                let Symbol::Constructor(ctor) = self.resolve(module, path)? else {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "pattern name is not a constructor",
                    ));
                };
                if !matches!(self.catalog.core.ty(data::backend_type(value_type)), Type::Enum(id,_) if *id == ctor.enum_id)
                {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "constructor pattern has the wrong value type",
                    ));
                }
                let declaration = self.catalog.declaration(ctor.enum_id).clone();
                let fields = &declaration.ast.variants[ctor.variant].fields;
                let types = self.catalog.fields(ctor, value_type);
                let mut checked = types
                    .iter()
                    .map(|&ty| ir::Pattern {
                        kind: ir::PatternKind::Any,
                        ty,
                        span,
                    })
                    .collect::<Vec<_>>();
                let mut assigned = HashSet::new();
                let mut named = false;
                for (index, argument) in args.iter().enumerate() {
                    let parameter = if let Some(label) = &argument.label {
                        named = true;
                        fields
                            .iter()
                            .position(|f| f.name.as_ref().is_some_and(|n| n.value == label.value))
                            .ok_or_else(|| {
                                self.error(module, DK::Name, label.span, "unknown pattern field")
                            })?
                    } else {
                        if named {
                            return Err(self.error(
                                module,
                                DK::Type,
                                argument.value.span,
                                "positional patterns must precede named patterns",
                            ));
                        }
                        index
                    };
                    if parameter >= fields.len() {
                        return Err(self.error(
                            module,
                            DK::Type,
                            span,
                            "too many constructor pattern fields",
                        ));
                    }
                    if !assigned.insert(parameter) {
                        return Err(self.error(
                            module,
                            DK::Name,
                            argument.value.span,
                            "duplicate constructor pattern field",
                        ));
                    }
                    checked[parameter] =
                        self.pattern(module, &argument.value, types[parameter], locals, names)?;
                }
                if !rest && assigned.len() != fields.len() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "constructor pattern omits fields; write '..' to ignore the rest",
                    ));
                }
                ir::PatternKind::Constructor {
                    ctor,
                    fields: checked,
                }
            }
        };
        Ok(ir::Pattern {
            kind,
            ty: value_type,
            span,
        })
    }

    fn construct(
        &mut self,
        module: usize,
        ctor: crate::contract::ir::CtorRef,
        args: &[ast::Argument],
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        use crate::contract::ir::{Type, TypeId};
        let declaration = self.catalog.declaration(ctor.enum_id).clone();
        let fields = &declaration.ast.variants[ctor.variant].fields;
        let raw_types = self.catalog.core.enums[ctor.enum_id].variants[ctor.variant]
            .tys
            .clone();
        let mut bindings = vec![None; declaration.ast.params.len()];
        if let Some(expected) = expected {
            if let Type::Enum(id, arguments) = self.catalog.core.ty(data::backend_type(expected)) {
                if *id == ctor.enum_id {
                    bindings = arguments.iter().copied().map(Some).collect();
                }
            }
        }
        let mut order = vec![usize::MAX; fields.len()];
        let mut named = false;
        for (index, argument) in args.iter().enumerate() {
            let parameter = if let Some(label) = &argument.label {
                named = true;
                fields
                    .iter()
                    .position(|f| f.name.as_ref().is_some_and(|n| n.value == label.value))
                    .ok_or_else(|| {
                        self.error(
                            module,
                            DK::Type,
                            label.span,
                            format!("unknown constructor field '{}'", label.value),
                        )
                    })?
            } else {
                if named {
                    return Err(self.error(
                        module,
                        DK::Type,
                        argument.value.span,
                        "positional arguments must precede named arguments",
                    ));
                }
                if index >= fields.len() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "too many constructor arguments",
                    ));
                }
                index
            };
            if order[parameter] != usize::MAX {
                return Err(self.error(
                    module,
                    DK::Type,
                    argument.value.span,
                    "constructor field is supplied twice",
                ));
            }
            order[parameter] = index;
        }
        if order.contains(&usize::MAX) {
            return Err(self.error(
                module,
                DK::Type,
                span,
                format!(
                    "constructor expects {} arguments, found {}",
                    fields.len(),
                    args.len()
                ),
            ));
        }
        for (parameter, &index) in order.iter().enumerate() {
            if let Some(hint) = self.hint(module, &args[index].value, locals) {
                self.infer_constructor(
                    module,
                    raw_types[parameter],
                    data::backend_type(hint),
                    &mut bindings,
                    span,
                )?;
            }
        }
        let mut checked = Vec::new();
        for (index, argument) in args.iter().enumerate() {
            let parameter = order
                .iter()
                .position(|&position| position == index)
                .unwrap();
            let substitutions = self.constructor_arguments(module, &mut bindings, span)?;
            let field_type = self
                .catalog
                .core
                .types
                .subst(raw_types[parameter], &substitutions);
            let hint = Some(data::frontend_type(field_type));
            let value = self.expr(module, &argument.value, locals, hint)?;
            self.infer_constructor(
                module,
                raw_types[parameter],
                data::backend_type(value.ty),
                &mut bindings,
                argument.value.span,
            )?;
            checked.push(value);
        }
        let substitutions = bindings
            .into_iter()
            .map(|binding| match binding {
                Some(ty) => Ok(ty),
                None => self
                    .solver
                    .try_fresh(&mut self.catalog.core.types, super::infer::Constraint::Any),
            })
            .collect::<Result<Vec<TypeId>, _>>()
            .map_err(|e| self.error(module, DK::Type, span, e.message))?;
        let value_type = Ty::Compound(
            self.catalog
                .core
                .types
                .enum_type(ctor.enum_id, substitutions),
        );
        if let Some(expected) = expected {
            self.expect_type(module, value_type, expected, span)?;
        }
        let value_type = self.resolve_type(module, value_type, span)?;
        self.validate_type_bounds(
            module,
            data::backend_type(value_type),
            &self.current_bounds,
            span,
        )?;
        Ok(ir::Expr {
            kind: ir::ExprKind::Make {
                ctor,
                args: checked,
                order,
            },
            ty: value_type,
            span,
        })
    }
    fn constructor_arguments(
        &mut self,
        module: usize,
        bindings: &mut [Option<crate::contract::ir::TypeId>],
        span: Span,
    ) -> Result<Vec<crate::contract::ir::TypeId>, Diagnostic> {
        bindings
            .iter_mut()
            .map(|binding| {
                if let Some(ty) = *binding {
                    return Ok(ty);
                }
                let ty = self
                    .solver
                    .try_fresh(&mut self.catalog.core.types, super::infer::Constraint::Any)
                    .map_err(|e| self.error(module, DK::Type, span, e.message))?;
                *binding = Some(ty);
                Ok(ty)
            })
            .collect()
    }
    fn infer_constructor(
        &mut self,
        module: usize,
        template: crate::contract::ir::TypeId,
        actual: crate::contract::ir::TypeId,
        bindings: &mut [Option<crate::contract::ir::TypeId>],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let arguments = self.constructor_arguments(module, bindings, span)?;
        let expected = self.catalog.core.types.subst(template, &arguments);
        self.expect_type(
            module,
            data::frontend_type(actual),
            data::frontend_type(expected),
            span,
        )
    }

    fn resolve(&self, module: usize, path: &[ast::Text]) -> Result<Symbol, Diagnostic> {
        let first = &path[0];
        if path.len() == 1 {
            if let Some(id) = self.module_definitions[module].get(&first.value) {
                return Ok(Symbol::Function(*id));
            }
            if let Some(symbol) = self.scopes[module].values.get(&first.value) {
                return Ok(*symbol);
            }
            if let Some(prelude) = self
                .modules
                .modules
                .iter()
                .position(|m| m.builtin && m.name == "Prelude")
            {
                if let Some(id) = self.module_definitions[prelude].get(&first.value) {
                    return Ok(Symbol::Function(*id));
                }
            }
        } else if path.len() == 2 {
            let target = self.scopes[module]
                .modules
                .get(&first.value)
                .copied()
                .or_else(|| {
                    self.modules
                        .modules
                        .iter()
                        .position(|m| m.builtin && m.name == first.value)
                });
            if let Some(target) = target {
                if let Some(id) = self.module_definitions[target].get(&path[1].value) {
                    if target != module && !self.definitions[id.0].ast.public {
                        return Err(self.error(
                            module,
                            DK::Name,
                            path[1].span,
                            format!("'{}.{}' is private", first.value, path[1].value),
                        ));
                    }
                    return Ok(Symbol::Function(*id));
                }
                if self.modules.modules[target].builtin {
                    if let Some(id) = constant(&self.modules.modules[target].name, &path[1].value) {
                        return Ok(Symbol::Constant(id));
                    }
                }
            }
        }
        let full_name = path
            .iter()
            .map(|p| p.value.as_str())
            .collect::<Vec<_>>()
            .join(".");
        if let Some(ctor) = self
            .catalog
            .constructor(&self.modules, module, &full_name, first.span)
            .map_err(|e| e.in_source(&self.modules.modules[module].source_name))?
        {
            return Ok(Symbol::Constructor(ctor));
        }
        Err(self.error(
            module,
            DK::Name,
            Span::new(first.span.start, path.last().unwrap().span.end),
            format!(
                "undefined name '{}'",
                path.iter()
                    .map(|p| p.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            ),
        ))
    }
    fn hint(&self, module: usize, expression: &ast::Expr, locals: &Locals) -> Option<Ty> {
        match &expression.kind {
            ast::ExprKind::Float(_) => Some(Ty::Float),
            ast::ExprKind::String(_) | ast::ExprKind::Interpolate(_) => Some(Ty::String),
            ast::ExprKind::Bool(_) => Some(Ty::Bool),
            ast::ExprKind::Name(path) => {
                if let Some(local) = (path.len() == 1)
                    .then(|| locals.names.get(&path[0].value))
                    .flatten()
                {
                    return Some(local.ty);
                }
                match self.resolve(module, path).ok()? {
                    Symbol::Constant(id) => Some(id.ty()),
                    Symbol::Function(_) | Symbol::Constructor(_) => None,
                }
            }
            ast::ExprKind::Call { callee, .. } => match &callee.kind {
                ast::ExprKind::Name(path) => match self.resolve(module, path).ok()? {
                    Symbol::Function(id) => self.definitions[id.0]
                        .signature
                        .ret
                        .filter(|ty| !self.catalog.core.types.has_param(data::backend_type(*ty))),
                    Symbol::Constant(_) | Symbol::Constructor(_) => None,
                },
                _ => None,
            },
            ast::ExprKind::Unary { value, .. } => self.hint(module, value, locals),
            ast::ExprKind::Binary { op, left, right } if !op.comparison() => self
                .hint(module, left, locals)
                .or_else(|| self.hint(module, right, locals)),
            _ => None,
        }
    }
    fn error(&self, module: usize, kind: DK, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(kind, span, message).in_source(&self.modules.modules[module].source_name)
    }
}
fn constant(module: &str, name: &str) -> Option<ConstantId> {
    match (module, name) {
        ("Int", "MIN") => Some(ConstantId::IntMin),
        ("Int", "MAX") => Some(ConstantId::IntMax),
        ("Int64", "MIN") => Some(ConstantId::Int64Min),
        ("Int64", "MAX") => Some(ConstantId::Int64Max),
        _ => None,
    }
}
fn valid_export_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(
            name,
            "auto"
                | "break"
                | "case"
                | "char"
                | "const"
                | "continue"
                | "default"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extern"
                | "float"
                | "for"
                | "goto"
                | "if"
                | "inline"
                | "int"
                | "long"
                | "register"
                | "restrict"
                | "return"
                | "short"
                | "signed"
                | "sizeof"
                | "static"
                | "struct"
                | "switch"
                | "typedef"
                | "union"
                | "unsigned"
                | "void"
                | "volatile"
                | "while"
                | "main"
        )
        && !((name.starts_with("int") || name.starts_with("uint")) && name.ends_with("_t"))
        && ![
            "emela_",
            "EMELA_",
            "INT",
            "UINT",
            "SIZE_",
            "PTRDIFF_",
            "SIG_ATOMIC_",
            "WCHAR_",
            "WINT_",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}
