//! Trait declarations, coherence and instance lookup over frontend-owned definitions.
//! Source collection, method signature checking and generated method bodies stay in the checker.
#![allow(dead_code)] // The checker connects this independent registry in the next integration commit.

use super::ir::DefinitionId;
use crate::contract::ir::{Definitions, EnumId, Type, TypeId};
use std::collections::{HashMap, HashSet};

pub(crate) type TraitId = usize;
pub(crate) type ParamBounds = HashMap<TypeId, Vec<TraitId>>;
#[derive(Clone, Debug)]
pub(crate) struct Method {
    pub(crate) name: String,
    /// A function signature whose rigid Param(0) denotes Self.
    pub(crate) signature: TypeId,
    pub(crate) default_function: Option<DefinitionId>,
}
#[derive(Clone, Debug)]
pub(crate) struct Trait {
    pub(crate) name: String,
    pub(crate) owner_module: usize,
    pub(crate) supers: Vec<TraitId>,
    pub(crate) methods: Vec<Method>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PrimitiveType {
    Int,
    Int64,
    Float,
    Bool,
    String,
    Unit,
    Never,
}
impl PrimitiveType {
    fn ty(self) -> TypeId {
        match self {
            Self::Int => TypeId::INT,
            Self::Int64 => TypeId::INT64,
            Self::Float => TypeId::FLOAT,
            Self::Bool => TypeId::BOOL,
            Self::String => TypeId::STRING,
            Self::Unit => TypeId::UNIT,
            Self::Never => TypeId::NEVER,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TypeHead {
    Primitive(PrimitiveType),
    Enum(EnumId),
}
#[derive(Clone, Debug)]
pub(crate) struct Impl {
    pub(crate) head: TypeHead,
    pub(crate) trait_id: TraitId,
    pub(crate) owner_module: usize,
    /// Bounds for each nominal type argument, in the declaration's parameter order.
    pub(crate) type_parameters: Vec<Vec<TraitId>>,
    pub(crate) methods: HashMap<String, DefinitionId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum BuiltinTrait {
    Eq,
    Ord,
    Show,
}
impl BuiltinTrait {
    fn method(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ord => "compare",
            Self::Show => "show",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedMethod {
    pub(crate) definition: Option<DefinitionId>,
    pub(crate) type_arguments: Vec<TypeId>,
    pub(crate) trait_id: TraitId,
    pub(crate) builtin: Option<BuiltinTrait>,
    pub(crate) from_default: bool,
    pub(crate) deferred: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TraitErrorKind {
    UnknownTrait,
    DuplicateTrait,
    DuplicateMethod,
    UnknownMethod,
    SupertraitCycle,
    OrphanImpl,
    OverlappingImpl,
    MissingMethod,
    UnsatisfiedBound,
    AmbiguousMethod,
    InvalidType,
    InvalidImpl,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TraitError {
    pub(crate) kind: TraitErrorKind,
    pub(crate) message: String,
}
impl std::fmt::Display for TraitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for TraitError {}
type Result<T> = std::result::Result<T, TraitError>;
fn error(kind: TraitErrorKind, message: impl Into<String>) -> TraitError {
    TraitError {
        kind,
        message: message.into(),
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Registry {
    traits: Vec<Trait>,
    names: HashMap<(usize, String), TraitId>,
    implementations: Vec<Impl>,
    type_owners: HashMap<EnumId, usize>,
    builtins: HashMap<BuiltinTrait, TraitId>,
    builtin_methods: HashMap<(BuiltinTrait, String), DefinitionId>,
    option_type: Option<EnumId>,
}
impl Registry {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn trait_data(&self, id: TraitId) -> Result<&Trait> {
        self.traits.get(id).ok_or_else(|| {
            error(
                TraitErrorKind::UnknownTrait,
                format!("unknown trait ID {id}"),
            )
        })
    }
    pub(crate) fn find_trait(&self, owner_module: usize, name: &str) -> Option<TraitId> {
        self.names.get(&(owner_module, name.into())).copied()
    }
    fn check_methods(methods: &[Method]) -> Result<()> {
        let mut seen = HashSet::new();
        for method in methods {
            if !seen.insert(&method.name) {
                return Err(error(
                    TraitErrorKind::DuplicateMethod,
                    format!("duplicate trait method `{}`", method.name),
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn add_trait(&mut self, definition: Trait) -> Result<TraitId> {
        let key = (definition.owner_module, definition.name.clone());
        if self.names.contains_key(&key) {
            return Err(error(
                TraitErrorKind::DuplicateTrait,
                format!("duplicate trait `{}`", definition.name),
            ));
        }
        Self::check_methods(&definition.methods)?;
        for &supertrait in &definition.supers {
            self.trait_data(supertrait)?;
        }
        let id = self.traits.len();
        self.names.insert(key, id);
        self.traits.push(definition);
        Ok(id)
    }
    /// Two-pass collection can register names first, then attach forward supertrait edges.
    pub(crate) fn set_supers(&mut self, id: TraitId, supers: Vec<TraitId>) -> Result<()> {
        self.trait_data(id)?;
        for &supertrait in &supers {
            self.trait_data(supertrait)?;
        }
        let previous = std::mem::replace(&mut self.traits[id].supers, supers);
        if let Err(error) = self.supertraits(id) {
            self.traits[id].supers = previous;
            return Err(error);
        }
        Ok(())
    }
    pub(crate) fn set_methods(&mut self, id: TraitId, methods: Vec<Method>) -> Result<()> {
        self.trait_data(id)?;
        Self::check_methods(&methods)?;
        self.traits[id].methods = methods;
        Ok(())
    }
    pub(crate) fn set_builtin(&mut self, id: TraitId, kind: BuiltinTrait) -> Result<()> {
        self.trait_data(id)?;
        if self
            .builtins
            .get(&kind)
            .is_some_and(|&existing| existing != id)
            || self
                .builtins
                .iter()
                .any(|(&other, &existing)| existing == id && other != kind)
        {
            return Err(error(
                TraitErrorKind::InvalidImpl,
                "builtin trait identity is already registered",
            ));
        }
        self.builtins.insert(kind, id);
        Ok(())
    }
    pub(crate) fn builtin_id(&self, kind: BuiltinTrait) -> Option<TraitId> {
        self.builtins.get(&kind).copied()
    }
    pub(crate) fn register_builtin_method(
        &mut self,
        kind: BuiltinTrait,
        name: impl Into<String>,
        definition: DefinitionId,
    ) {
        self.builtin_methods.insert((kind, name.into()), definition);
    }
    pub(crate) fn register_type_owner(&mut self, id: EnumId, module: usize) -> Result<()> {
        if self
            .type_owners
            .get(&id)
            .is_some_and(|&existing| existing != module)
        {
            return Err(error(
                TraitErrorKind::InvalidImpl,
                "nominal type owner was registered twice",
            ));
        }
        self.type_owners.insert(id, module);
        Ok(())
    }
    pub(crate) fn register_option_type(&mut self, core: &Definitions, id: EnumId) -> Result<()> {
        let definition = core
            .enums
            .iter()
            .find_map(|(key, value)| (key == id).then_some(value))
            .ok_or_else(|| {
                error(
                    TraitErrorKind::InvalidType,
                    "invalid compiler-owned Option type ID",
                )
            })?;
        let valid = definition.params == 1
            && definition.variants.len() == 2
            && definition
                .variants
                .iter()
                .any(|v| v.name == "None" && v.tys.is_empty())
            && definition.variants.iter().any(|v| {
                v.name == "Some"
                    && v.tys.len() == 1
                    && matches!(core.types.types.get(v.tys[0].index()), Some(Type::Param(0)))
            });
        if !valid {
            return Err(error(
                TraitErrorKind::InvalidType,
                "compiler-owned Option declaration has an invalid shape",
            ));
        }
        if self.option_type.is_some_and(|existing| existing != id) {
            return Err(error(
                TraitErrorKind::InvalidType,
                "compiler-owned Option identity was registered twice",
            ));
        }
        self.option_type = Some(id);
        Ok(())
    }
    pub(crate) fn supertraits(&self, id: TraitId) -> Result<Vec<TraitId>> {
        fn visit(
            registry: &Registry,
            id: TraitId,
            active: &mut HashSet<TraitId>,
            seen: &mut HashSet<TraitId>,
        ) -> Result<()> {
            registry.trait_data(id)?;
            if active.contains(&id) {
                return Err(error(
                    TraitErrorKind::SupertraitCycle,
                    "supertrait declarations contain a cycle",
                ));
            }
            if !seen.insert(id) {
                return Ok(());
            }
            active.insert(id);
            for &parent in &registry.traits[id].supers {
                visit(registry, parent, active, seen)?;
            }
            active.remove(&id);
            Ok(())
        }
        let mut seen = HashSet::new();
        visit(self, id, &mut HashSet::new(), &mut seen)?;
        seen.remove(&id);
        let mut ids: Vec<_> = seen.into_iter().collect();
        ids.sort_unstable();
        Ok(ids)
    }
    fn builtin(&self, id: TraitId) -> Option<BuiltinTrait> {
        self.builtins
            .iter()
            .find_map(|(&kind, &key)| (key == id).then_some(kind))
    }
    fn node(core: &Definitions, ty: TypeId) -> Result<&Type> {
        core.types.types.get(ty.index()).ok_or_else(|| {
            error(
                TraitErrorKind::InvalidType,
                "type ID is outside the shared catalog",
            )
        })
    }
    pub(crate) fn head_of(core: &Definitions, ty: TypeId) -> Result<Option<TypeHead>> {
        Ok(match Self::node(core, ty)? {
            Type::Int => Some(TypeHead::Primitive(PrimitiveType::Int)),
            Type::Int64 => Some(TypeHead::Primitive(PrimitiveType::Int64)),
            Type::Float => Some(TypeHead::Primitive(PrimitiveType::Float)),
            Type::Bool => Some(TypeHead::Primitive(PrimitiveType::Bool)),
            Type::String => Some(TypeHead::Primitive(PrimitiveType::String)),
            Type::Unit => Some(TypeHead::Primitive(PrimitiveType::Unit)),
            Type::Never => Some(TypeHead::Primitive(PrimitiveType::Never)),
            Type::Enum(id, _) => Some(TypeHead::Enum(*id)),
            _ => None,
        })
    }
    pub(crate) fn add_impl(&mut self, core: &Definitions, implementation: Impl) -> Result<()> {
        let definition = self.trait_data(implementation.trait_id)?;
        for bounds in &implementation.type_parameters {
            for &bound in bounds {
                self.trait_data(bound)?;
            }
        }
        let (parameters, type_is_local) = match implementation.head {
            TypeHead::Primitive(_) => (0, false),
            TypeHead::Enum(id) => {
                let definition = core
                    .enums
                    .iter()
                    .find_map(|(key, value)| (key == id).then_some(value))
                    .ok_or_else(|| {
                        error(
                            TraitErrorKind::InvalidType,
                            "impl head references an unknown nominal type",
                        )
                    })?;
                (
                    definition.params,
                    self.type_owners.get(&id) == Some(&implementation.owner_module),
                )
            }
        };
        if parameters != implementation.type_parameters.len() {
            return Err(error(
                TraitErrorKind::InvalidImpl,
                "impl type parameter count differs from its nominal head",
            ));
        }
        if implementation.owner_module != definition.owner_module && !type_is_local {
            return Err(error(
                TraitErrorKind::OrphanImpl,
                "impl must belong to the trait's module or the nominal type's module",
            ));
        }
        if self.builtin(implementation.trait_id) == Some(BuiltinTrait::Ord)
            && implementation.head == TypeHead::Primitive(PrimitiveType::Float)
        {
            return Err(error(
                TraitErrorKind::InvalidImpl,
                "Float has no Ord instance",
            ));
        }
        if self.implementations.iter().any(|existing| {
            existing.trait_id == implementation.trait_id && existing.head == implementation.head
        }) {
            return Err(error(
                TraitErrorKind::OverlappingImpl,
                "impl overlaps an existing trait instance for the same type head",
            ));
        }
        let mut expected: HashMap<_, _> = definition
            .methods
            .iter()
            .map(|method| (method.name.as_str(), method.default_function.is_some()))
            .collect();
        if let Some(kind) = self.builtin(implementation.trait_id) {
            expected.entry(kind.method()).or_insert(false);
        }
        for method in implementation.methods.keys() {
            if !expected.contains_key(method.as_str()) {
                return Err(error(
                    TraitErrorKind::UnknownMethod,
                    format!("trait has no method `{method}`"),
                ));
            }
        }
        for (method, has_default) in expected {
            if !has_default && !implementation.methods.contains_key(method) {
                return Err(error(
                    TraitErrorKind::MissingMethod,
                    format!("impl is missing required method `{method}`"),
                ));
            }
        }
        self.implementations.push(implementation);
        Ok(())
    }
    /// Run after all impls are collected, allowing source-order independent supertrait impls.
    pub(crate) fn validate_impls(&self, core: &mut Definitions) -> Result<()> {
        for implementation in &self.implementations {
            let args: Vec<_> = (0..implementation.type_parameters.len())
                .map(|index| core.types.intern(Type::Param(index)))
                .collect();
            let ty = match implementation.head {
                TypeHead::Primitive(kind) => kind.ty(),
                TypeHead::Enum(id) => core.types.intern(Type::Enum(id, args.clone())),
            };
            let bounds: ParamBounds = args
                .into_iter()
                .zip(implementation.type_parameters.clone())
                .collect();
            if !self.satisfies(core, ty, implementation.trait_id, &bounds)? {
                return Err(error(
                    TraitErrorKind::UnsatisfiedBound,
                    "impl does not satisfy its declared bounds and supertraits",
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn satisfies(
        &self,
        core: &Definitions,
        ty: TypeId,
        trait_id: TraitId,
        bounds: &ParamBounds,
    ) -> Result<bool> {
        self.satisfies_inner(core, ty, trait_id, bounds, &mut HashSet::new())
    }
    fn satisfies_inner(
        &self,
        core: &Definitions,
        ty: TypeId,
        trait_id: TraitId,
        bounds: &ParamBounds,
        active: &mut HashSet<(TypeId, TraitId)>,
    ) -> Result<bool> {
        self.trait_data(trait_id)?;
        let node = Self::node(core, ty)?;
        if !active.insert((ty, trait_id)) {
            return Ok(false);
        }
        let result = (|| {
            if matches!(node, Type::Param(_)) {
                for &available in bounds.get(&ty).into_iter().flatten() {
                    if available == trait_id || self.supertraits(available)?.contains(&trait_id) {
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
            if let Some(head) = Self::head_of(core, ty)? {
                if let Some(implementation) = self.implementations.iter().find(|implementation| {
                    implementation.head == head && implementation.trait_id == trait_id
                }) {
                    let args = match node {
                        Type::Enum(_, args) => args.as_slice(),
                        _ => &[],
                    };
                    if args.len() != implementation.type_parameters.len() {
                        return Err(error(
                            TraitErrorKind::InvalidType,
                            "instance type argument count differs from declaration",
                        ));
                    }
                    for (&argument, required) in args.iter().zip(&implementation.type_parameters) {
                        for &required in required {
                            if !self.satisfies_inner(core, argument, required, bounds, active)? {
                                return Ok(false);
                            }
                        }
                    }
                    for &parent in &self.traits[trait_id].supers {
                        if !self.satisfies_inner(core, ty, parent, bounds, active)? {
                            return Ok(false);
                        }
                    }
                    return Ok(true);
                }
            }
            let Some(kind) = self.builtin(trait_id) else {
                return Ok(false);
            };
            let supported = match node {
                Type::Int | Type::Int64 | Type::Bool | Type::String | Type::Unit => true,
                Type::Float => kind != BuiltinTrait::Ord,
                Type::Tuple(fields) => {
                    let mut all = true;
                    for &field in fields {
                        all &= self.satisfies_inner(core, field, trait_id, bounds, active)?;
                    }
                    all
                }
                Type::Enum(id, args) if self.option_type == Some(*id) => {
                    args.len() == 1
                        && self.satisfies_inner(core, args[0], trait_id, bounds, active)?
                }
                _ => false,
            };
            if !supported {
                return Ok(false);
            }
            for &parent in &self.traits[trait_id].supers {
                if !self.satisfies_inner(core, ty, parent, bounds, active)? {
                    return Ok(false);
                }
            }
            Ok(true)
        })();
        active.remove(&(ty, trait_id));
        result
    }
    fn method_owner(&self, trait_id: TraitId, name: &str) -> Result<TraitId> {
        let definition = self.trait_data(trait_id)?;
        if definition.methods.iter().any(|method| method.name == name)
            || self
                .builtin(trait_id)
                .is_some_and(|kind| kind.method() == name)
        {
            return Ok(trait_id);
        }
        let mut owners = Vec::new();
        for parent in self.supertraits(trait_id)? {
            let definition = &self.traits[parent];
            if definition.methods.iter().any(|method| method.name == name)
                || self
                    .builtin(parent)
                    .is_some_and(|kind| kind.method() == name)
            {
                owners.push(parent);
            }
        }
        // A method overridden by a nearer supertrait hides its ancestor's method.
        // Unrelated supertraits with the same method name remain ambiguous.
        let mut hidden = HashSet::new();
        for &owner in &owners {
            hidden.extend(self.supertraits(owner)?);
        }
        owners.retain(|owner| !hidden.contains(owner));
        match owners.as_slice() {
            [owner] => Ok(*owner),
            [] => Err(error(
                TraitErrorKind::UnknownMethod,
                format!("trait has no method `{name}`"),
            )),
            _ => Err(error(
                TraitErrorKind::AmbiguousMethod,
                format!("method `{name}` is inherited from multiple traits"),
            )),
        }
    }
    pub(crate) fn resolve(
        &self,
        core: &Definitions,
        ty: TypeId,
        trait_id: TraitId,
        name: &str,
        bounds: &ParamBounds,
    ) -> Result<ResolvedMethod> {
        let owner = self.method_owner(trait_id, name)?;
        if !self.satisfies(core, ty, trait_id, bounds)? {
            return Err(error(
                TraitErrorKind::UnsatisfiedBound,
                format!(
                    "type {ty:?} does not implement `{}`",
                    self.traits[trait_id].name
                ),
            ));
        }
        if matches!(Self::node(core, ty)?, Type::Param(_)) {
            return Ok(ResolvedMethod {
                definition: None,
                type_arguments: vec![ty],
                trait_id: owner,
                builtin: self.builtin(owner),
                from_default: false,
                deferred: true,
            });
        }
        if let Some(head) = Self::head_of(core, ty)? {
            if let Some(implementation) = self.implementations.iter().find(|implementation| {
                implementation.head == head && implementation.trait_id == owner
            }) {
                if let Some(&definition) = implementation.methods.get(name) {
                    let arguments = match Self::node(core, ty)? {
                        Type::Enum(_, args) => args.clone(),
                        _ => vec![],
                    };
                    return Ok(ResolvedMethod {
                        definition: Some(definition),
                        type_arguments: arguments,
                        trait_id: owner,
                        builtin: None,
                        from_default: false,
                        deferred: false,
                    });
                }
                if let Some(definition) = self.traits[owner]
                    .methods
                    .iter()
                    .find(|method| method.name == name)
                    .and_then(|method| method.default_function)
                {
                    return Ok(ResolvedMethod {
                        definition: Some(definition),
                        type_arguments: vec![ty],
                        trait_id: owner,
                        builtin: None,
                        from_default: true,
                        deferred: false,
                    });
                }
            }
        }
        if let Some(kind) = self.builtin(owner) {
            return Ok(ResolvedMethod {
                definition: self.builtin_methods.get(&(kind, name.into())).copied(),
                type_arguments: vec![ty],
                trait_id: owner,
                builtin: Some(kind),
                from_default: false,
                deferred: false,
            });
        }
        Err(error(
            TraitErrorKind::MissingMethod,
            format!("instance has no implementation for `{name}`"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ir::{EnumDef, VariantDef, VariantFields};
    fn trait_(name: &str, owner: usize, methods: Vec<Method>) -> Trait {
        Trait {
            name: name.into(),
            owner_module: owner,
            supers: vec![],
            methods,
        }
    }
    fn method(name: &str, default: Option<usize>) -> Method {
        Method {
            name: name.into(),
            signature: TypeId::UNIT,
            default_function: default.map(DefinitionId),
        }
    }
    fn nominal(core: &mut Definitions, name: &str, params: usize) -> EnumId {
        core.enums.alloc(EnumDef {
            name: name.into(),
            params,
            variants: vec![],
            error: false,
            external: false,
        })
    }
    fn builtins() -> (Registry, TraitId, TraitId, TraitId) {
        let mut registry = Registry::new();
        let eq = registry.add_trait(trait_("Eq", 0, vec![])).unwrap();
        let ord = registry.add_trait(trait_("Ord", 0, vec![])).unwrap();
        let show = registry.add_trait(trait_("Show", 0, vec![])).unwrap();
        registry.set_builtin(eq, BuiltinTrait::Eq).unwrap();
        registry.set_builtin(ord, BuiltinTrait::Ord).unwrap();
        registry.set_builtin(show, BuiltinTrait::Show).unwrap();
        registry.set_supers(ord, vec![eq]).unwrap();
        (registry, eq, ord, show)
    }
    #[test]
    fn custom_method_and_trait_default_resolve_like_score_fixture() {
        let mut core = Definitions::default();
        let key = nominal(&mut core, "Key", 0);
        let ty = core.types.intern(Type::Enum(key, vec![]));
        let mut registry = Registry::new();
        let score = registry
            .add_trait(trait_(
                "Score",
                1,
                vec![method("score", None), method("next", Some(11))],
            ))
            .unwrap();
        registry.register_type_owner(key, 2).unwrap();
        registry
            .add_impl(
                &core,
                Impl {
                    head: TypeHead::Enum(key),
                    trait_id: score,
                    owner_module: 2,
                    type_parameters: vec![],
                    methods: HashMap::from([("score".into(), DefinitionId(10))]),
                },
            )
            .unwrap();
        let actual = registry
            .resolve(&core, ty, score, "score", &ParamBounds::new())
            .unwrap();
        assert_eq!(actual.definition, Some(DefinitionId(10)));
        assert!(!actual.from_default);
        let next = registry
            .resolve(&core, ty, score, "next", &ParamBounds::new())
            .unwrap();
        assert_eq!(next.definition, Some(DefinitionId(11)));
        assert_eq!(next.type_arguments, vec![ty]);
        assert!(next.from_default);
    }
    #[test]
    fn coherence_rejects_orphans_overlap_missing_and_unknown_methods() {
        let mut core = Definitions::default();
        let key = nominal(&mut core, "Key", 0);
        let mut registry = Registry::new();
        let score = registry
            .add_trait(trait_("Score", 1, vec![method("score", None)]))
            .unwrap();
        registry.register_type_owner(key, 2).unwrap();
        let implementation = Impl {
            head: TypeHead::Enum(key),
            trait_id: score,
            owner_module: 3,
            type_parameters: vec![],
            methods: HashMap::from([("score".into(), DefinitionId(10))]),
        };
        assert_eq!(
            registry
                .add_impl(&core, implementation.clone())
                .unwrap_err()
                .kind,
            TraitErrorKind::OrphanImpl
        );
        let mut implementation = implementation;
        implementation.owner_module = 2;
        let mut missing = implementation.clone();
        missing.methods.clear();
        assert_eq!(
            registry.add_impl(&core, missing).unwrap_err().kind,
            TraitErrorKind::MissingMethod
        );
        let mut unknown = implementation.clone();
        unknown.methods.insert("typo".into(), DefinitionId(12));
        assert_eq!(
            registry.add_impl(&core, unknown).unwrap_err().kind,
            TraitErrorKind::UnknownMethod
        );
        registry.add_impl(&core, implementation.clone()).unwrap();
        assert_eq!(
            registry.add_impl(&core, implementation).unwrap_err().kind,
            TraitErrorKind::OverlappingImpl
        );
    }
    #[test]
    fn supertrait_cycles_rollback_and_bounds_include_transitive_supers() {
        let mut registry = Registry::new();
        let a = registry
            .add_trait(trait_("A", 1, vec![method("a", None)]))
            .unwrap();
        let b = registry.add_trait(trait_("B", 1, vec![])).unwrap();
        let c = registry.add_trait(trait_("C", 1, vec![])).unwrap();
        registry.set_supers(b, vec![a]).unwrap();
        registry.set_supers(c, vec![b]).unwrap();
        assert_eq!(
            registry.set_supers(a, vec![c]).unwrap_err().kind,
            TraitErrorKind::SupertraitCycle
        );
        assert!(registry.supertraits(a).unwrap().is_empty());
        let mut core = Definitions::default();
        let parameter = core.types.intern(Type::Param(0));
        let bounds = HashMap::from([(parameter, vec![c])]);
        assert!(registry.satisfies(&core, parameter, a, &bounds).unwrap());
        assert!(
            registry
                .resolve(&core, parameter, c, "a", &bounds)
                .unwrap()
                .deferred
        );
    }
    #[test]
    fn builtin_instances_are_identity_based_and_float_is_not_ordered() {
        let mut core = Definitions::default();
        let (mut registry, eq, ord, show) = builtins();
        let tuple = core
            .types
            .intern(Type::Tuple(vec![TypeId::INT, TypeId::FLOAT]));
        assert!(
            registry
                .satisfies(&core, tuple, eq, &ParamBounds::new())
                .unwrap()
        );
        assert!(
            registry
                .satisfies(&core, tuple, show, &ParamBounds::new())
                .unwrap()
        );
        assert!(
            !registry
                .satisfies(&core, tuple, ord, &ParamBounds::new())
                .unwrap()
        );
        let fake = registry
            .add_trait(trait_("Eq", 2, vec![method("eq", None)]))
            .unwrap();
        assert!(
            !registry
                .satisfies(&core, TypeId::INT, fake, &ParamBounds::new())
                .unwrap()
        );
        let op = registry
            .resolve(&core, TypeId::FLOAT, eq, "eq", &ParamBounds::new())
            .unwrap();
        assert_eq!(op.builtin, Some(BuiltinTrait::Eq));
        assert_eq!(
            registry
                .add_impl(
                    &core,
                    Impl {
                        head: TypeHead::Primitive(PrimitiveType::Float),
                        trait_id: ord,
                        owner_module: 0,
                        type_parameters: vec![],
                        methods: HashMap::from([("compare".into(), DefinitionId(1))])
                    }
                )
                .unwrap_err()
                .kind,
            TraitErrorKind::InvalidImpl
        );
    }
    #[test]
    fn generic_instance_bounds_discharge_and_defaults_are_specialized() {
        let mut core = Definitions::default();
        let (mut registry, eq, _, _) = builtins();
        let wrapper = nominal(&mut core, "Envelope", 1);
        registry.register_type_owner(wrapper, 2).unwrap();
        registry
            .add_impl(
                &core,
                Impl {
                    head: TypeHead::Enum(wrapper),
                    trait_id: eq,
                    owner_module: 2,
                    type_parameters: vec![vec![eq]],
                    methods: HashMap::from([("eq".into(), DefinitionId(7))]),
                },
            )
            .unwrap();
        let int = core.types.intern(Type::Enum(wrapper, vec![TypeId::INT]));
        let unsupported = core.types.intern(Type::Fn(crate::contract::ir::FnType {
            params: vec![],
            ret: TypeId::UNIT,
            fails: false,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let bad = core.types.intern(Type::Enum(wrapper, vec![unsupported]));
        assert!(
            registry
                .satisfies(&core, int, eq, &ParamBounds::new())
                .unwrap()
        );
        assert!(
            !registry
                .satisfies(&core, bad, eq, &ParamBounds::new())
                .unwrap()
        );
        let resolved = registry
            .resolve(&core, int, eq, "eq", &ParamBounds::new())
            .unwrap();
        assert_eq!(resolved.type_arguments, vec![TypeId::INT]);
        registry.validate_impls(&mut core).unwrap();
    }
    #[test]
    fn option_builtin_requires_the_compiler_owned_identity() {
        let mut core = Definitions::default();
        let (mut registry, eq, ord, _) = builtins();
        let parameter = core.types.intern(Type::Param(0));
        let option = core.enums.alloc(EnumDef {
            name: "Option".into(),
            params: 1,
            variants: vec![
                VariantDef {
                    name: "None".into(),
                    fields: VariantFields::Unit,
                    tys: vec![],
                },
                VariantDef {
                    name: "Some".into(),
                    fields: VariantFields::Positional(1),
                    tys: vec![parameter],
                },
            ],
            error: false,
            external: false,
        });
        let int = core.types.intern(Type::Enum(option, vec![TypeId::INT]));
        let float = core.types.intern(Type::Enum(option, vec![TypeId::FLOAT]));
        assert!(
            !registry
                .satisfies(&core, int, eq, &ParamBounds::new())
                .unwrap()
        );
        registry.register_option_type(&core, option).unwrap();
        assert!(
            registry
                .satisfies(&core, int, ord, &ParamBounds::new())
                .unwrap()
        );
        assert!(
            !registry
                .satisfies(&core, float, ord, &ParamBounds::new())
                .unwrap()
        );
    }
    #[test]
    fn supertrait_instances_are_required_after_source_order_independent_collection() {
        let mut core = Definitions::default();
        let key = nominal(&mut core, "Key", 0);
        let mut registry = Registry::new();
        let a = registry
            .add_trait(trait_("A", 1, vec![method("a", None)]))
            .unwrap();
        let b = registry.add_trait(trait_("B", 1, vec![])).unwrap();
        registry.set_supers(b, vec![a]).unwrap();
        registry.register_type_owner(key, 2).unwrap();
        registry
            .add_impl(
                &core,
                Impl {
                    head: TypeHead::Enum(key),
                    trait_id: b,
                    owner_module: 2,
                    type_parameters: vec![],
                    methods: HashMap::new(),
                },
            )
            .unwrap();
        assert_eq!(
            registry.validate_impls(&mut core).unwrap_err().kind,
            TraitErrorKind::UnsatisfiedBound
        );
        registry
            .add_impl(
                &core,
                Impl {
                    head: TypeHead::Enum(key),
                    trait_id: a,
                    owner_module: 2,
                    type_parameters: vec![],
                    methods: HashMap::from([("a".into(), DefinitionId(5))]),
                },
            )
            .unwrap();
        registry.validate_impls(&mut core).unwrap();
    }
}
