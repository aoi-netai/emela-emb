//! Unification and top-level schemes over the frontend's shared type catalog.
//!
//! Inference parameters use unused indices allocated downwards from `usize::MAX`.
//! Identity is determined by the explicit `metas` registry, never by an index range.
//! Source declarations must create rigid parameters through `register_rigid_parameter`;
//! a late rigid declaration cannot silently reuse an inference parameter's identity.
#![allow(dead_code)] // The checker adopts this independent solver in the next integration commit.

use crate::contract::ir::{FnSets, Type, TypeId, TypeTable};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Constraint {
    Any,
    Numeric,
    Integer,
}
impl Constraint {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Integer, _) | (_, Self::Integer) => Self::Integer,
            (Self::Numeric, _) | (_, Self::Numeric) => Self::Numeric,
            _ => Self::Any,
        }
    }
    fn accepts(self, ty: &Type) -> bool {
        match self {
            Self::Any => true,
            Self::Numeric => matches!(ty, Type::Int | Type::Int64 | Type::Float),
            Self::Integer => matches!(ty, Type::Int | Type::Int64),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeErrorKind {
    Mismatch,
    Constraint,
    OccursCheck,
    InvalidType,
    ParameterCollision,
    ParameterExhausted,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TypeError {
    pub(crate) kind: TypeErrorKind,
    pub(crate) actual: TypeId,
    pub(crate) expected: TypeId,
    pub(crate) message: String,
}
impl std::fmt::Display for TypeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for TypeError {}
fn error(
    kind: TypeErrorKind,
    actual: TypeId,
    expected: TypeId,
    message: impl Into<String>,
) -> TypeError {
    TypeError {
        kind,
        actual,
        expected,
        message: message.into(),
    }
}
type Result<T> = std::result::Result<T, TypeError>;

#[derive(Debug, Clone)]
struct Meta {
    constraint: Constraint,
    binding: Option<TypeId>,
}
/// Quantified parameters are rigid, freshly allocated catalog entries. They are not
/// mutable inference variables and remain stable when the solver continues checking.
#[derive(Debug, Clone)]
pub(crate) struct Scheme {
    pub(crate) ty: TypeId,
    pub(crate) params: Vec<TypeId>,
    pub(crate) constraints: Vec<Constraint>,
}
#[derive(Debug)]
pub(crate) struct Solver {
    metas: HashMap<TypeId, Meta>,
    allocated_parameters: HashSet<usize>,
    next_parameter: Option<usize>,
}
impl Default for Solver {
    fn default() -> Self {
        Self {
            metas: HashMap::new(),
            allocated_parameters: HashSet::new(),
            next_parameter: Some(usize::MAX),
        }
    }
}
impl Solver {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    fn parameter(&mut self, types: &mut TypeTable) -> Result<TypeId> {
        self.allocated_parameters
            .extend(types.types.iter().filter_map(|ty| match ty {
                Type::Param(index) => Some(*index),
                _ => None,
            }));
        loop {
            let index = self.next_parameter.ok_or_else(|| {
                error(
                    TypeErrorKind::ParameterExhausted,
                    TypeId::UNIT,
                    TypeId::UNIT,
                    "type parameter namespace is exhausted",
                )
            })?;
            self.next_parameter = index.checked_sub(1);
            if self.allocated_parameters.insert(index) {
                return Ok(types.intern(Type::Param(index)));
            }
        }
    }
    pub(crate) fn try_fresh(
        &mut self,
        types: &mut TypeTable,
        constraint: Constraint,
    ) -> Result<TypeId> {
        let id = self.parameter(types)?;
        self.metas.insert(
            id,
            Meta {
                constraint,
                binding: None,
            },
        );
        Ok(id)
    }
    /// The catalog has u32 IDs, so exhausting the usize parameter namespace is unreachable
    /// before its own allocation limit. `try_fresh` exposes the explicit boundary check.
    pub(crate) fn fresh(&mut self, types: &mut TypeTable, constraint: Constraint) -> TypeId {
        self.try_fresh(types, constraint)
            .expect("type parameter allocation stays within the catalog limit")
    }
    pub(crate) fn register_rigid_parameter(
        &mut self,
        types: &mut TypeTable,
        index: usize,
    ) -> Result<TypeId> {
        let id = types.intern(Type::Param(index));
        if self.metas.contains_key(&id) {
            return Err(error(
                TypeErrorKind::ParameterCollision,
                id,
                id,
                "rigid type parameter collides with a registered inference variable",
            ));
        }
        self.allocated_parameters.insert(index);
        Ok(id)
    }
    pub(crate) fn is_meta(&self, ty: TypeId) -> bool {
        self.metas.contains_key(&ty)
    }
    pub(crate) fn constraint(&self, ty: TypeId) -> Option<Constraint> {
        self.metas.get(&ty).map(|meta| meta.constraint)
    }
    fn node(types: &TypeTable, ty: TypeId) -> Result<Type> {
        types.types.get(ty.index()).cloned().ok_or_else(|| {
            error(
                TypeErrorKind::InvalidType,
                ty,
                ty,
                "type ID is outside the shared catalog",
            )
        })
    }
    pub(crate) fn resolve(&mut self, types: &mut TypeTable, ty: TypeId) -> Result<TypeId> {
        self.resolve_inner(types, ty, &mut HashSet::new())
    }
    fn resolve_inner(
        &mut self,
        types: &mut TypeTable,
        ty: TypeId,
        active: &mut HashSet<TypeId>,
    ) -> Result<TypeId> {
        if !active.insert(ty) {
            return Err(error(
                TypeErrorKind::OccursCheck,
                ty,
                ty,
                "cyclic type binding",
            ));
        }
        let result = if let Some(bound) = self.metas.get(&ty).and_then(|meta| meta.binding) {
            let resolved = self.resolve_inner(types, bound, active)?;
            self.metas
                .get_mut(&ty)
                .expect("registered variable")
                .binding = Some(resolved);
            resolved
        } else {
            let node = Self::node(types, ty)?;
            let mapped = self.map_node(types, node, |solver, types, child| {
                solver.resolve_inner(types, child, active)
            })?;
            types.intern(mapped)
        };
        active.remove(&ty);
        Ok(result)
    }
    fn map_node(
        &mut self,
        types: &mut TypeTable,
        node: Type,
        mut map: impl FnMut(&mut Self, &mut TypeTable, TypeId) -> Result<TypeId>,
    ) -> Result<Type> {
        Ok(match node {
            Type::Tuple(fields) => Type::Tuple(
                fields
                    .into_iter()
                    .map(|ty| map(self, types, ty))
                    .collect::<Result<_>>()?,
            ),
            Type::Enum(id, args) => Type::Enum(
                id,
                args.into_iter()
                    .map(|ty| map(self, types, ty))
                    .collect::<Result<_>>()?,
            ),
            Type::List(element) => Type::List(map(self, types, element)?),
            Type::Cell(element) => Type::Cell(map(self, types, element)?),
            Type::Fn(mut signature) => {
                signature.params = signature
                    .params
                    .into_iter()
                    .map(|ty| map(self, types, ty))
                    .collect::<Result<_>>()?;
                signature.ret = map(self, types, signature.ret)?;
                signature.sets = signature
                    .sets
                    .map(|sets| {
                        Ok(FnSets {
                            errors: map(self, types, sets.errors)?,
                            effects: map(self, types, sets.effects)?,
                        })
                    })
                    .transpose()?;
                Type::Fn(signature)
            }
            Type::Set(mut summary) => {
                summary.params = summary
                    .params
                    .into_iter()
                    .map(|ty| map(self, types, ty))
                    .collect::<Result<_>>()?;
                Type::Set(summary)
            }
            scalar => scalar,
        })
    }
    /// Directional checking: a diverging actual expression is compatible with any
    /// expected type, without forcing an expected metavariable to become Never.
    pub(crate) fn unify(
        &mut self,
        types: &mut TypeTable,
        actual: TypeId,
        expected: TypeId,
    ) -> Result<()> {
        let actual = self.resolve(types, actual)?;
        let expected = self.resolve(types, expected)?;
        if actual == expected || actual == TypeId::NEVER {
            return Ok(());
        }
        if self.metas.contains_key(&actual) {
            return self.bind(types, actual, expected);
        }
        if self.metas.contains_key(&expected) {
            return self.bind(types, expected, actual);
        }
        match (Self::node(types, actual)?, Self::node(types, expected)?) {
            (Type::Tuple(a), Type::Tuple(b)) => self.unify_many(types, &a, &b, actual, expected),
            (Type::Enum(a, args_a), Type::Enum(b, args_b)) if a == b => {
                self.unify_many(types, &args_a, &args_b, actual, expected)
            }
            (Type::List(a), Type::List(b)) | (Type::Cell(a), Type::Cell(b)) => {
                self.unify(types, a, b)
            }
            (Type::Fn(a), Type::Fn(b))
                if a.fails == b.fails && a.suspends == b.suspends && a.ev == b.ev =>
            {
                self.unify_many(types, &a.params, &b.params, actual, expected)?;
                self.unify(types, a.ret, b.ret)?;
                match (a.sets, b.sets) {
                    (Some(a), Some(b)) => {
                        self.unify(types, a.errors, b.errors)?;
                        self.unify(types, a.effects, b.effects)
                    }
                    (None, None) => Ok(()),
                    _ => Err(error(
                        TypeErrorKind::Mismatch,
                        actual,
                        expected,
                        "function set metadata differs",
                    )),
                }
            }
            (Type::Set(a), Type::Set(b))
                if a.fails == b.fails && a.suspends == b.suspends && a.ev == b.ev =>
            {
                self.unify_many(types, &a.params, &b.params, actual, expected)
            }
            _ => Err(error(
                TypeErrorKind::Mismatch,
                actual,
                expected,
                format!("cannot unify {actual:?} with {expected:?}"),
            )),
        }
    }
    fn unify_many(
        &mut self,
        types: &mut TypeTable,
        actual: &[TypeId],
        expected: &[TypeId],
        a: TypeId,
        b: TypeId,
    ) -> Result<()> {
        if actual.len() != expected.len() {
            return Err(error(
                TypeErrorKind::Mismatch,
                a,
                b,
                "type argument or parameter counts differ",
            ));
        }
        for (&actual, &expected) in actual.iter().zip(expected) {
            self.unify(types, actual, expected)?;
        }
        Ok(())
    }
    fn bind(&mut self, types: &mut TypeTable, variable: TypeId, target: TypeId) -> Result<()> {
        if variable == target {
            return Ok(());
        }
        let constraint = self.metas[&variable].constraint;
        if let Some(other) = self.metas.get_mut(&target) {
            other.constraint = other.constraint.merge(constraint);
            self.metas
                .get_mut(&variable)
                .expect("registered variable")
                .binding = Some(target);
            return Ok(());
        }
        if !constraint.accepts(&Self::node(types, target)?) {
            return Err(error(
                TypeErrorKind::Constraint,
                target,
                variable,
                format!("type {target:?} does not satisfy {constraint:?}"),
            ));
        }
        if self.free_metas(types, target)?.contains(&variable) {
            return Err(error(
                TypeErrorKind::OccursCheck,
                variable,
                target,
                "inference variable occurs in its own type",
            ));
        }
        self.metas
            .get_mut(&variable)
            .expect("registered variable")
            .binding = Some(target);
        Ok(())
    }
    pub(crate) fn free_metas(
        &mut self,
        types: &mut TypeTable,
        ty: TypeId,
    ) -> Result<HashSet<TypeId>> {
        let resolved = self.resolve(types, ty)?;
        let mut result = HashSet::new();
        let mut seen = HashSet::new();
        let mut pending = vec![resolved];
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if self
                .metas
                .get(&ty)
                .is_some_and(|meta| meta.binding.is_none())
            {
                result.insert(ty);
            }
            match Self::node(types, ty)? {
                Type::Tuple(fields) | Type::Enum(_, fields) => pending.extend(fields),
                Type::List(element) | Type::Cell(element) => pending.push(element),
                Type::Fn(signature) => {
                    pending.extend(signature.params);
                    pending.push(signature.ret);
                    if let Some(sets) = signature.sets {
                        pending.extend([sets.errors, sets.effects]);
                    }
                }
                Type::Set(summary) => pending.extend(summary.params),
                _ => {}
            }
        }
        Ok(result)
    }
    fn default_metas(
        &mut self,
        types: &mut TypeTable,
        variables: impl IntoIterator<Item = TypeId>,
    ) -> Result<()> {
        for variable in variables {
            let resolved = self.resolve(types, variable)?;
            if self
                .metas
                .get(&resolved)
                .is_some_and(|meta| meta.binding.is_none() && meta.constraint != Constraint::Any)
            {
                self.bind(types, resolved, TypeId::INT)?;
            }
        }
        Ok(())
    }
    pub(crate) fn finish_defaults(&mut self, types: &mut TypeTable) -> Result<()> {
        self.default_metas(types, self.metas.keys().copied().collect::<Vec<_>>())
    }
    pub(crate) fn substitute(
        &mut self,
        types: &mut TypeTable,
        ty: TypeId,
        substitutions: &HashMap<TypeId, TypeId>,
    ) -> Result<TypeId> {
        let ty = self.resolve(types, ty)?;
        if let Some(&replacement) = substitutions.get(&ty) {
            Self::node(types, replacement)?;
            return Ok(replacement);
        }
        let node = Self::node(types, ty)?;
        let node = self.map_node(types, node, |solver, types, child| {
            solver.substitute(types, child, substitutions)
        })?;
        Ok(types.intern(node))
    }
    pub(crate) fn generalize(
        &mut self,
        types: &mut TypeTable,
        ty: TypeId,
        environment: &HashSet<TypeId>,
    ) -> Result<Scheme> {
        let variables = self.free_metas(types, ty)?;
        self.default_metas(types, variables)?;
        let mut excluded = HashSet::new();
        for &ty in environment {
            excluded.extend(self.free_metas(types, ty)?);
        }
        let mut variables: Vec<_> = self
            .free_metas(types, ty)?
            .difference(&excluded)
            .copied()
            .collect();
        variables.sort_by_key(|id| id.index());
        let mut substitutions = HashMap::new();
        let mut params = Vec::new();
        let mut constraints = Vec::new();
        for variable in variables {
            let rigid = self.parameter(types)?;
            substitutions.insert(variable, rigid);
            params.push(rigid);
            constraints.push(self.metas[&variable].constraint);
        }
        let ty = self.substitute(types, ty, &substitutions)?;
        // Freeze the original quantified variables too. The definition's inferred signature
        // cannot later be mutated through an alias that predates generalization.
        for (variable, rigid) in substitutions {
            self.metas
                .get_mut(&variable)
                .expect("quantified metavariable")
                .binding = Some(rigid);
        }
        Ok(Scheme {
            ty,
            params,
            constraints,
        })
    }
    pub(crate) fn instantiate(&mut self, types: &mut TypeTable, scheme: &Scheme) -> Result<TypeId> {
        if scheme.params.len() != scheme.constraints.len() {
            return Err(error(
                TypeErrorKind::InvalidType,
                scheme.ty,
                scheme.ty,
                "scheme parameter and constraint counts differ",
            ));
        }
        let mut substitutions = HashMap::new();
        for (&parameter, &constraint) in scheme.params.iter().zip(&scheme.constraints) {
            if !matches!(Self::node(types, parameter)?, Type::Param(_))
                || self.metas.contains_key(&parameter)
            {
                return Err(error(
                    TypeErrorKind::ParameterCollision,
                    parameter,
                    parameter,
                    "quantified scheme parameter is not rigid",
                ));
            }
            substitutions.insert(parameter, self.try_fresh(types, constraint)?);
        }
        self.substitute(types, scheme.ty, &substitutions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ir::{FnType, SetSummary};
    fn function(types: &mut TypeTable, params: Vec<TypeId>, ret: TypeId) -> TypeId {
        types.intern(Type::Fn(FnType {
            params,
            ret,
            fails: false,
            suspends: false,
            ev: false,
            sets: None,
        }))
    }
    #[test]
    fn scalar_numeric_integer_constraints_and_defaults_are_separate() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let numeric = solver.fresh(&mut types, Constraint::Numeric);
        solver.unify(&mut types, TypeId::FLOAT, numeric).unwrap();
        assert_eq!(solver.resolve(&mut types, numeric).unwrap(), TypeId::FLOAT);
        let integer = solver.fresh(&mut types, Constraint::Integer);
        assert_eq!(
            solver
                .unify(&mut types, TypeId::FLOAT, integer)
                .unwrap_err()
                .kind,
            TypeErrorKind::Constraint
        );
        solver.unify(&mut types, TypeId::INT64, integer).unwrap();
        let default = solver.fresh(&mut types, Constraint::Integer);
        solver.finish_defaults(&mut types).unwrap();
        assert_eq!(solver.resolve(&mut types, default).unwrap(), TypeId::INT);
    }
    #[test]
    fn unification_propagates_constraints_and_checks_occurs() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let a = solver.fresh(&mut types, Constraint::Any);
        let b = solver.fresh(&mut types, Constraint::Numeric);
        solver.unify(&mut types, a, b).unwrap();
        assert_eq!(
            solver.unify(&mut types, TypeId::BOOL, a).unwrap_err().kind,
            TypeErrorKind::Constraint
        );
        let a = solver.fresh(&mut types, Constraint::Any);
        let list = types.intern(Type::List(a));
        assert_eq!(
            solver.unify(&mut types, a, list).unwrap_err().kind,
            TypeErrorKind::OccursCheck
        );
        assert_eq!(solver.resolve(&mut types, a).unwrap(), a);
    }
    #[test]
    fn never_actual_does_not_force_an_inferred_result_type() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let result = solver.fresh(&mut types, Constraint::Any);
        solver.unify(&mut types, TypeId::NEVER, result).unwrap();
        assert_eq!(solver.resolve(&mut types, result).unwrap(), result);
        solver.unify(&mut types, TypeId::STRING, result).unwrap();
        assert_eq!(solver.resolve(&mut types, result).unwrap(), TypeId::STRING);
        assert_eq!(
            solver
                .unify(&mut types, TypeId::INT, TypeId::NEVER)
                .unwrap_err()
                .kind,
            TypeErrorKind::Mismatch
        );
    }
    #[test]
    fn generic_identity_instantiations_have_independent_variables() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let a = solver.fresh(&mut types, Constraint::Any);
        let signature = function(&mut types, vec![a], a);
        let scheme = solver
            .generalize(&mut types, signature, &HashSet::new())
            .unwrap();
        assert_eq!(scheme.params.len(), 1);
        let first = solver.instantiate(&mut types, &scheme).unwrap();
        let second = solver.instantiate(&mut types, &scheme).unwrap();
        let concrete = function(&mut types, vec![TypeId::INT], TypeId::INT);
        solver.unify(&mut types, first, concrete).unwrap();
        let concrete = function(&mut types, vec![TypeId::STRING], TypeId::STRING);
        solver.unify(&mut types, second, concrete).unwrap();
        assert_ne!(
            solver.resolve(&mut types, first).unwrap(),
            solver.resolve(&mut types, second).unwrap()
        );
        assert_eq!(solver.resolve(&mut types, signature).unwrap(), scheme.ty);
    }
    #[test]
    fn generalization_excludes_environment_and_defaults_numeric_variables() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let shared = solver.fresh(&mut types, Constraint::Any);
        let local = solver.fresh(&mut types, Constraint::Any);
        let integer = solver.fresh(&mut types, Constraint::Integer);
        let tuple = types.intern(Type::Tuple(vec![shared, local, integer]));
        let scheme = solver
            .generalize(&mut types, tuple, &HashSet::from([shared]))
            .unwrap();
        assert_eq!(scheme.params.len(), 1);
        let Type::Tuple(fields) = &types[scheme.ty] else {
            panic!()
        };
        assert_eq!(fields[0], shared);
        assert_eq!(fields[2], TypeId::INT);
    }
    #[test]
    fn rigid_parameters_and_meta_registry_cannot_silently_collide() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let high = solver
            .register_rigid_parameter(&mut types, usize::MAX)
            .unwrap();
        let ordinary = solver.register_rigid_parameter(&mut types, 0).unwrap();
        let meta = solver.fresh(&mut types, Constraint::Any);
        assert_ne!(meta, high);
        assert!(!solver.is_meta(ordinary));
        assert!(!solver.is_meta(high));
        let Type::Param(index) = types[meta] else {
            panic!()
        };
        assert_eq!(
            solver
                .register_rigid_parameter(&mut types, index)
                .unwrap_err()
                .kind,
            TypeErrorKind::ParameterCollision
        );
        solver.next_parameter = None;
        assert_eq!(
            solver
                .try_fresh(&mut types, Constraint::Any)
                .unwrap_err()
                .kind,
            TypeErrorKind::ParameterExhausted
        );
    }
    #[test]
    fn function_value_types_and_invocation_sets_unify_independently() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let value = solver.fresh(&mut types, Constraint::Any);
        let error = solver.fresh(&mut types, Constraint::Any);
        let effect = solver.fresh(&mut types, Constraint::Any);
        let empty = types.intern(Type::Set(SetSummary::default()));
        let signature = types.intern(Type::Fn(FnType {
            params: vec![value],
            ret: value,
            fails: false,
            suspends: false,
            ev: false,
            sets: Some(FnSets {
                errors: error,
                effects: effect,
            }),
        }));
        let scheme = solver
            .generalize(&mut types, signature, &HashSet::new())
            .unwrap();
        assert_eq!(scheme.params.len(), 3);
        let instance = solver.instantiate(&mut types, &scheme).unwrap();
        let concrete = types.intern(Type::Fn(FnType {
            params: vec![TypeId::BOOL],
            ret: TypeId::BOOL,
            fails: false,
            suspends: false,
            ev: false,
            sets: Some(FnSets {
                errors: empty,
                effects: empty,
            }),
        }));
        solver.unify(&mut types, instance, concrete).unwrap();
        assert_eq!(solver.resolve(&mut types, instance).unwrap(), concrete);
    }
    #[test]
    fn malformed_catalog_ids_return_a_typed_error() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let missing = TypeId::from_index(10000);
        assert_eq!(
            solver.resolve(&mut types, missing).unwrap_err().kind,
            TypeErrorKind::InvalidType
        );
    }
}
