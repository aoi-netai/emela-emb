//! Error/effect row constraints, independent of syntax and the value-type solver.
//! Difference removes explicit tags only: a variable keeps its original meaning.
use crate::contract::ir::TypeId;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetKind {
    Error,
    Effect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct VarId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Inferred,
    HiddenParameter,
    OpaqueParameter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tag {
    pub head: usize,
    pub args: Vec<TypeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub kind: SetKind,
    pub tags: Vec<Tag>,
    pub vars: Vec<VarId>,
}

impl Row {
    pub(crate) fn empty(kind: SetKind) -> Self {
        Self {
            kind,
            tags: vec![],
            vars: vec![],
        }
    }

    pub(crate) fn variable(kind: SetKind, var: VarId) -> Self {
        Self {
            kind,
            tags: vec![],
            vars: vec![var],
        }
    }

    pub(crate) fn tag(kind: SetKind, tag: Tag) -> Self {
        Self {
            kind,
            tags: vec![tag],
            vars: vec![],
        }
    }

    pub(crate) fn difference(&self, heads: &[usize]) -> Self {
        let mut result = self.clone();
        result.tags.retain(|tag| !heads.contains(&tag.head));
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SetError {
    WrongKind { actual: SetKind, expected: SetKind },
    UnknownVariable(VarId),
    TagArity { head: usize },
    Type(String),
    MissingTag { kind: SetKind, head: usize },
    MissingVariable(VarId),
    NotImmediateTag(usize),
    NotImmediateVariable(VarId),
}

type Result<T> = std::result::Result<T, SetError>;
type Unify<'a> = dyn FnMut(TypeId, TypeId) -> std::result::Result<(), String> + 'a;

#[derive(Clone, Debug)]
struct Variable {
    kind: SetKind,
    origin: Origin,
    lower: Row,
    upper_bounded: bool,
    immediate: bool,
    quantified: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Quantified {
    pub variable: VarId,
    pub kind: SetKind,
    pub immediate: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Solver {
    variables: Vec<Variable>,
    inclusions: Vec<(Row, Row)>,
    immediate_rows: Vec<Row>,
}

impl Solver {
    pub(crate) fn map_tags(&mut self, map: &mut dyn FnMut(&Tag) -> Tag) {
        fn row(row: &mut Row, map: &mut dyn FnMut(&Tag) -> Tag) {
            row.tags = row.tags.iter().map(map).collect();
        }
        for variable in &mut self.variables {
            row(&mut variable.lower, map);
        }
        for (actual, expected) in &mut self.inclusions {
            row(actual, map);
            row(expected, map);
        }
        for immediate in &mut self.immediate_rows {
            row(immediate, map);
        }
    }
    pub(crate) fn fresh(&mut self, kind: SetKind, origin: Origin) -> VarId {
        let id = VarId(self.variables.len());
        self.variables.push(Variable {
            kind,
            origin,
            lower: Row::empty(kind),
            upper_bounded: false,
            immediate: false,
            quantified: false,
        });
        id
    }

    fn variable(&self, id: VarId) -> Result<&Variable> {
        self.variables
            .get(id.0)
            .ok_or(SetError::UnknownVariable(id))
    }

    fn validate(&self, row: &Row) -> Result<()> {
        for &id in &row.vars {
            same_kind(self.variable(id)?.kind, row.kind)?;
        }
        Ok(())
    }

    pub(crate) fn union(&self, left: &Row, right: &Row, unify: &mut Unify<'_>) -> Result<Row> {
        self.validate(left)?;
        self.validate(right)?;
        let mut result = Row::empty(left.kind);
        merge(&mut result, left, unify)?;
        merge(&mut result, right, unify)?;
        Ok(result)
    }

    /// Constrain actual to be contained in expected. Function parameter positions
    /// reverse these operands; invariant named type arguments add both directions.
    pub(crate) fn includes(&mut self, actual: Row, expected: Row) -> Result<()> {
        self.validate(&actual)?;
        self.validate(&expected)?;
        same_kind(actual.kind, expected.kind)?;
        if actual == expected
            || (actual.tags.is_empty() && actual.vars.is_empty())
            || self
                .inclusions
                .iter()
                .any(|(left, right)| *left == actual && *right == expected)
        {
            return Ok(());
        }
        self.inclusions.push((actual, expected));
        Ok(())
    }

    pub(crate) fn equal(&mut self, left: Row, right: Row) -> Result<()> {
        self.includes(left.clone(), right.clone())?;
        self.includes(right, left)
    }

    /// Record a declared bound (including the implicit bound of an external's
    /// hidden callback parameter). A use-site requirement is a different operation.
    pub(crate) fn declare_immediate(&mut self, id: VarId) -> Result<()> {
        same_kind(self.variable(id)?.kind, SetKind::Effect)?;
        self.variables[id.0].immediate = true;
        Ok(())
    }

    pub(crate) fn require_immediate(&mut self, row: Row) -> Result<()> {
        self.validate(&row)?;
        same_kind(row.kind, SetKind::Effect)?;
        self.immediate_rows.push(row);
        Ok(())
    }

    fn flexible(&self, id: VarId) -> bool {
        self.variables[id.0].origin == Origin::Inferred && !self.variables[id.0].quantified
    }

    fn extend_lower(&mut self, id: VarId, row: &Row, unify: &mut Unify<'_>) -> Result<bool> {
        let mut addition = row.clone();
        addition.vars.retain(|other| *other != id);
        merge(&mut self.variables[id.0].lower, &addition, unify)
    }

    fn expand(
        &self,
        row: &Row,
        keep_flexible: bool,
        seen: &mut HashSet<VarId>,
        unify: &mut Unify<'_>,
    ) -> Result<Row> {
        self.validate(row)?;
        let mut result = Row::empty(row.kind);
        for tag in &row.tags {
            merge(&mut result, &Row::tag(row.kind, tag.clone()), unify)?;
        }
        for &id in &row.vars {
            let variable = self.variable(id)?;
            if (!self.flexible(id) || keep_flexible) && !result.vars.contains(&id) {
                result.vars.push(id);
            }
            if seen.insert(id) {
                let lower = self.expand(&variable.lower, keep_flexible, seen, unify)?;
                merge(&mut result, &lower, unify)?;
                seen.remove(&id);
            }
        }
        Ok(result)
    }

    pub(crate) fn resolve(&self, row: &Row, unify: &mut Unify<'_>) -> Result<Row> {
        self.expand(row, false, &mut HashSet::new(), unify)
    }

    /// Iterate monotone lower bounds. Unknown rigid variables are never silently
    /// replaced by the empty set. The first writable expected variable receives
    /// unmatched members, preserving the source specification's approximation.
    pub(crate) fn solve(
        &mut self,
        unify: &mut Unify<'_>,
        immediate: &mut dyn FnMut(&Tag) -> bool,
    ) -> Result<()> {
        loop {
            let mut changed = false;
            for (actual, expected) in self.inclusions.clone() {
                let actual = self.expand(&actual, true, &mut HashSet::new(), unify)?;
                let expected_expanded = self.expand(&expected, true, &mut HashSet::new(), unify)?;
                let destination = expected
                    .vars
                    .first()
                    .copied()
                    .filter(|id| self.flexible(*id));
                for tag in actual.tags {
                    if let Some(other) = expected_expanded
                        .tags
                        .iter()
                        .find(|other| other.head == tag.head)
                    {
                        unify_tags(&tag, other, unify)?;
                    } else if let Some(id) = destination {
                        changed |= self.extend_lower(id, &Row::tag(actual.kind, tag), unify)?;
                    } else {
                        return Err(SetError::MissingTag {
                            kind: actual.kind,
                            head: tag.head,
                        });
                    }
                }
                for id in actual.vars {
                    if expected_expanded.vars.contains(&id) {
                        continue;
                    }
                    if let Some(destination) = destination {
                        changed |=
                            self.extend_lower(destination, &Row::variable(actual.kind, id), unify)?;
                    } else if self.flexible(id) {
                        // The persistent inclusion checks all future lower bounds too.
                        self.variables[id.0].upper_bounded = true;
                    } else {
                        return Err(SetError::MissingVariable(id));
                    }
                }
            }
            let mut rows = self.immediate_rows.clone();
            rows.extend(
                self.variables
                    .iter()
                    .filter(|v| v.immediate)
                    .map(|v| v.lower.clone()),
            );
            for row in rows {
                let expanded = self.expand(&row, true, &mut HashSet::new(), unify)?;
                for tag in &expanded.tags {
                    if !immediate(tag) {
                        return Err(SetError::NotImmediateTag(tag.head));
                    }
                }
                for id in expanded.vars {
                    if self.variables[id.0].immediate {
                        continue;
                    }
                    if !self.flexible(id) {
                        return Err(SetError::NotImmediateVariable(id));
                    }
                    self.variables[id.0].immediate = true;
                    changed = true;
                }
            }
            if !changed {
                return Ok(());
            }
        }
    }

    /// Quantify unbounded parameters outside the environment; constrained inferred
    /// rows instead resolve to the smallest set permitted by their lower bounds.
    pub(crate) fn generalize(&mut self, environment: &[VarId]) -> Result<Vec<Quantified>> {
        let candidates: Vec<_> = (0..self.variables.len()).map(VarId).collect();
        self.generalize_from(&candidates, environment)
    }

    /// The frontend supplies variables occurring in parameter function types.
    /// Other unconstrained expression rows keep their smallest (empty) solution.
    pub(crate) fn generalize_from(
        &mut self,
        candidates: &[VarId],
        environment: &[VarId],
    ) -> Result<Vec<Quantified>> {
        for &id in candidates {
            self.variable(id)?;
        }
        let eligible: HashSet<_> = candidates.iter().copied().collect();
        let mut excluded = HashSet::new();
        let mut pending = environment.to_vec();
        // If A flows into an upper-bounded B, A is bounded too. Quantifying A
        // after solving B <= {} would otherwise turn a proved pure row unknown.
        pending.extend(
            self.variables
                .iter()
                .enumerate()
                .filter_map(|(index, var)| var.upper_bounded.then_some(VarId(index))),
        );
        while let Some(id) = pending.pop() {
            let variable = self.variable(id)?;
            if excluded.insert(id) {
                pending.extend(&variable.lower.vars);
            }
        }
        let mut result = vec![];
        for (index, variable) in self.variables.iter_mut().enumerate() {
            let id = VarId(index);
            if !eligible.contains(&id)
                || excluded.contains(&id)
                || variable.origin == Origin::OpaqueParameter
                || variable.upper_bounded
                || !variable.lower.tags.is_empty()
                || !variable.lower.vars.is_empty()
                || variable.quantified
            {
                continue;
            }
            variable.quantified = true;
            result.push(Quantified {
                variable: id,
                kind: variable.kind,
                immediate: variable.immediate,
            });
        }
        // Mutual inclusion is equality. An unconstrained cycle needs one shared
        // quantified variable, rather than silently collapsing every member to {}.
        for index in 0..self.variables.len() {
            let id = VarId(index);
            if !eligible.contains(&id) {
                continue;
            }
            let reachable = self.dependencies(id);
            let group: HashSet<_> = reachable
                .iter()
                .copied()
                .filter(|member| self.dependencies(*member).contains(&id))
                .collect();
            if group.len() < 2
                || group.iter().any(|member| {
                    let variable = &self.variables[member.0];
                    excluded.contains(member)
                        || variable.origin != Origin::Inferred
                        || variable.upper_bounded
                        || variable.quantified
                        || !variable.lower.tags.is_empty()
                        || variable.lower.vars.iter().any(|next| !group.contains(next))
                })
            {
                continue;
            }
            self.variables[index].quantified = true;
            result.push(Quantified {
                variable: id,
                kind: self.variables[index].kind,
                immediate: self.variables[index].immediate,
            });
        }
        Ok(result)
    }

    fn dependencies(&self, root: VarId) -> HashSet<VarId> {
        let mut result = HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if result.insert(id) {
                pending.extend(&self.variables[id.0].lower.vars);
            }
        }
        result
    }

    /// Substitute quantified variables simultaneously. Replacement rows are not
    /// recursively substituted, so permutations and per-call fresh variables work.
    pub(crate) fn instantiate(
        &self,
        row: &Row,
        replacements: &HashMap<VarId, Row>,
        unify: &mut Unify<'_>,
    ) -> Result<Row> {
        self.validate(row)?;
        let mut result = Row::empty(row.kind);
        for tag in &row.tags {
            merge(&mut result, &Row::tag(row.kind, tag.clone()), unify)?;
        }
        for &id in &row.vars {
            if let Some(replacement) = replacements.get(&id) {
                self.validate(replacement)?;
                same_kind(row.kind, replacement.kind)?;
                merge(&mut result, replacement, unify)?;
            } else if !result.vars.contains(&id) {
                result.vars.push(id);
            }
        }
        Ok(result)
    }
}

fn same_kind(actual: SetKind, expected: SetKind) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(SetError::WrongKind { actual, expected })
    }
}

fn unify_tags(left: &Tag, right: &Tag, unify: &mut Unify<'_>) -> Result<()> {
    if left.args.len() != right.args.len() {
        return Err(SetError::TagArity { head: left.head });
    }
    for (&left, &right) in left.args.iter().zip(&right.args) {
        unify(left, right).map_err(SetError::Type)?;
    }
    Ok(())
}

fn merge(left: &mut Row, right: &Row, unify: &mut Unify<'_>) -> Result<bool> {
    same_kind(left.kind, right.kind)?;
    let mut changed = false;
    for tag in &right.tags {
        if let Some(other) = left.tags.iter().find(|other| other.head == tag.head) {
            unify_tags(other, tag, unify)?;
        } else {
            left.tags.push(tag.clone());
            changed = true;
        }
    }
    for var in &right.vars {
        if !left.vars.contains(var) {
            left.vars.push(*var);
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unify(left: TypeId, right: TypeId) -> std::result::Result<(), String> {
        if left == right {
            Ok(())
        } else {
            Err("payload mismatch".into())
        }
    }
    fn tag(kind: SetKind, head: usize) -> Row {
        Row::tag(kind, Tag { head, args: vec![] })
    }

    #[test]
    fn union_unifies_payloads_and_rejects_kind_or_arity_mismatches() {
        let solver = Solver::default();
        let left = Row::tag(
            SetKind::Error,
            Tag {
                head: 1,
                args: vec![TypeId::INT],
            },
        );
        assert_eq!(solver.union(&left, &left, &mut unify).unwrap(), left);
        let right = Row::tag(
            SetKind::Error,
            Tag {
                head: 1,
                args: vec![TypeId::STRING],
            },
        );
        assert!(matches!(
            solver.union(&left, &right, &mut unify),
            Err(SetError::Type(_))
        ));
        let duplicate = Row {
            kind: SetKind::Error,
            tags: vec![left.tags[0].clone(), right.tags[0].clone()],
            vars: vec![],
        };
        assert!(matches!(
            solver.resolve(&duplicate, &mut unify),
            Err(SetError::Type(_))
        ));
        assert!(matches!(
            solver.union(&left, &tag(SetKind::Error, 1), &mut unify),
            Err(SetError::TagArity { .. })
        ));
        assert!(matches!(
            solver.union(&left, &Row::empty(SetKind::Effect), &mut unify),
            Err(SetError::WrongKind { .. })
        ));
    }

    #[test]
    fn difference_does_not_remove_tags_later_discovered_through_a_variable() {
        let mut solver = Solver::default();
        let var = solver.fresh(SetKind::Error, Origin::Inferred);
        let row = solver
            .union(
                &tag(SetKind::Error, 7),
                &Row::variable(SetKind::Error, var),
                &mut unify,
            )
            .unwrap();
        let escaped = row.difference(&[7]);
        solver
            .includes(tag(SetKind::Error, 7), Row::variable(SetKind::Error, var))
            .unwrap();
        solver.solve(&mut unify, &mut |_| true).unwrap();
        assert_eq!(
            solver.resolve(&escaped, &mut unify).unwrap(),
            tag(SetKind::Error, 7)
        );
    }

    #[test]
    fn multiple_expected_variables_receive_missing_tags_in_source_order() {
        let mut solver = Solver::default();
        let a = solver.fresh(SetKind::Error, Origin::Inferred);
        let b = solver.fresh(SetKind::Error, Origin::Inferred);
        let expected = Row {
            kind: SetKind::Error,
            tags: tag(SetKind::Error, 1).tags,
            vars: vec![a, b],
        };
        solver
            .includes(
                solver
                    .union(&tag(SetKind::Error, 1), &tag(SetKind::Error, 2), &mut unify)
                    .unwrap(),
                expected,
            )
            .unwrap();
        solver.solve(&mut unify, &mut |_| true).unwrap();
        assert_eq!(
            solver
                .resolve(&Row::variable(SetKind::Error, a), &mut unify)
                .unwrap(),
            tag(SetKind::Error, 2)
        );
        assert_eq!(
            solver
                .resolve(&Row::variable(SetKind::Error, b), &mut unify)
                .unwrap(),
            Row::empty(SetKind::Error)
        );
    }

    #[test]
    fn recursive_inclusion_propagates_late_tags_and_checks_closed_upper_bounds() {
        let mut solver = Solver::default();
        let a = solver.fresh(SetKind::Error, Origin::Inferred);
        let b = solver.fresh(SetKind::Error, Origin::Inferred);
        solver
            .equal(
                Row::variable(SetKind::Error, a),
                Row::variable(SetKind::Error, b),
            )
            .unwrap();
        solver
            .includes(Row::variable(SetKind::Error, b), tag(SetKind::Error, 1))
            .unwrap();
        solver
            .includes(tag(SetKind::Error, 2), Row::variable(SetKind::Error, a))
            .unwrap();
        assert_eq!(
            solver.solve(&mut unify, &mut |_| true),
            Err(SetError::MissingTag {
                kind: SetKind::Error,
                head: 2
            })
        );
    }

    #[test]
    fn rigid_and_hidden_parameters_cannot_be_proved_empty_or_immediate() {
        for origin in [Origin::OpaqueParameter, Origin::HiddenParameter] {
            let mut solver = Solver::default();
            let var = solver.fresh(SetKind::Effect, origin);
            solver
                .includes(
                    Row::variable(SetKind::Effect, var),
                    Row::empty(SetKind::Effect),
                )
                .unwrap();
            assert_eq!(
                solver.solve(&mut unify, &mut |_| true),
                Err(SetError::MissingVariable(var))
            );
            let mut solver = Solver::default();
            let var = solver.fresh(SetKind::Effect, origin);
            solver
                .require_immediate(Row::variable(SetKind::Effect, var))
                .unwrap();
            assert_eq!(
                solver.solve(&mut unify, &mut |_| true),
                Err(SetError::NotImmediateVariable(var))
            );
            solver.declare_immediate(var).unwrap();
            solver.solve(&mut unify, &mut |_| true).unwrap();
        }
    }

    #[test]
    fn immediate_bounds_are_inferred_and_rechecked_after_instantiation() {
        let mut solver = Solver::default();
        let var = solver.fresh(SetKind::Effect, Origin::Inferred);
        solver
            .require_immediate(Row::variable(SetKind::Effect, var))
            .unwrap();
        solver.solve(&mut unify, &mut |_| true).unwrap();
        let quantified = solver.generalize(&[]).unwrap();
        assert_eq!(
            quantified,
            vec![Quantified {
                variable: var,
                kind: SetKind::Effect,
                immediate: true
            }]
        );
        let fresh = solver.fresh(SetKind::Effect, Origin::Inferred);
        let instantiated = solver
            .instantiate(
                &Row::variable(SetKind::Effect, var),
                &HashMap::from([(var, Row::variable(SetKind::Effect, fresh))]),
                &mut unify,
            )
            .unwrap();
        solver.require_immediate(instantiated.clone()).unwrap();
        solver
            .includes(tag(SetKind::Effect, 99), instantiated)
            .unwrap();
        assert_eq!(
            solver.solve(&mut unify, &mut |tag| tag.head != 99),
            Err(SetError::NotImmediateTag(99))
        );
    }

    #[test]
    fn generalization_preserves_the_environment_and_defaults_bounded_rows() {
        let mut solver = Solver::default();
        let environment = solver.fresh(SetKind::Error, Origin::Inferred);
        let hidden = solver.fresh(SetKind::Error, Origin::HiddenParameter);
        let opaque = solver.fresh(SetKind::Error, Origin::OpaqueParameter);
        let inferred = solver.fresh(SetKind::Error, Origin::Inferred);
        solver
            .includes(
                Row::variable(SetKind::Error, inferred),
                Row::empty(SetKind::Error),
            )
            .unwrap();
        solver.solve(&mut unify, &mut |_| true).unwrap();
        assert_eq!(
            solver
                .generalize(&[environment])
                .unwrap()
                .iter()
                .map(|q| q.variable)
                .collect::<Vec<_>>(),
            vec![hidden]
        );
        assert_eq!(
            solver
                .resolve(&Row::variable(SetKind::Error, inferred), &mut unify)
                .unwrap(),
            Row::empty(SetKind::Error)
        );
        assert_eq!(
            solver
                .resolve(&Row::variable(SetKind::Error, opaque), &mut unify)
                .unwrap(),
            Row::variable(SetKind::Error, opaque)
        );
    }

    #[test]
    fn simultaneous_instantiation_preserves_permutations_and_checks_kinds() {
        let mut solver = Solver::default();
        let a = solver.fresh(SetKind::Error, Origin::OpaqueParameter);
        let b = solver.fresh(SetKind::Error, Origin::OpaqueParameter);
        let row = Row {
            kind: SetKind::Error,
            tags: vec![],
            vars: vec![a, b],
        };
        let replacements = HashMap::from([
            (a, Row::variable(SetKind::Error, b)),
            (b, tag(SetKind::Error, 1)),
        ]);
        let got = solver.instantiate(&row, &replacements, &mut unify).unwrap();
        assert_eq!(got.vars, vec![b]);
        assert_eq!(got.tags, tag(SetKind::Error, 1).tags);
        assert!(matches!(
            solver.instantiate(
                &row,
                &HashMap::from([(a, Row::empty(SetKind::Effect))]),
                &mut unify
            ),
            Err(SetError::WrongKind { .. })
        ));
    }

    #[test]
    fn unconstrained_equality_cycles_generalize_once_and_share_instantiation() {
        let mut solver = Solver::default();
        let a = solver.fresh(SetKind::Error, Origin::Inferred);
        let b = solver.fresh(SetKind::Error, Origin::Inferred);
        solver
            .equal(
                Row::variable(SetKind::Error, a),
                Row::variable(SetKind::Error, b),
            )
            .unwrap();
        solver.solve(&mut unify, &mut |_| true).unwrap();
        let quantified = solver.generalize(&[]).unwrap();
        assert_eq!(quantified.len(), 1);
        let a = solver
            .resolve(&Row::variable(SetKind::Error, a), &mut unify)
            .unwrap();
        let b = solver
            .resolve(&Row::variable(SetKind::Error, b), &mut unify)
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(a.vars, vec![quantified[0].variable]);
        let replacements = HashMap::from([(quantified[0].variable, tag(SetKind::Error, 7))]);
        assert_eq!(
            solver.instantiate(&a, &replacements, &mut unify).unwrap(),
            tag(SetKind::Error, 7)
        );
    }

    #[test]
    fn only_parameter_function_rows_are_candidates_for_generalization() {
        let mut solver = Solver::default();
        let parameter = solver.fresh(SetKind::Effect, Origin::Inferred);
        let expression = solver.fresh(SetKind::Effect, Origin::Inferred);
        let quantified = solver.generalize_from(&[parameter], &[]).unwrap();
        assert_eq!(
            quantified.iter().map(|q| q.variable).collect::<Vec<_>>(),
            vec![parameter]
        );
        assert_eq!(
            solver
                .resolve(&Row::variable(SetKind::Effect, expression), &mut unify)
                .unwrap(),
            Row::empty(SetKind::Effect)
        );
    }

    #[test]
    fn an_upper_bound_propagates_back_through_inclusion_before_generalization() {
        let mut solver = Solver::default();
        let source = solver.fresh(SetKind::Error, Origin::Inferred);
        let result = solver.fresh(SetKind::Error, Origin::Inferred);
        solver
            .includes(
                Row::variable(SetKind::Error, source),
                Row::variable(SetKind::Error, result),
            )
            .unwrap();
        solver
            .includes(
                Row::variable(SetKind::Error, result),
                Row::empty(SetKind::Error),
            )
            .unwrap();
        solver.solve(&mut unify, &mut |_| true).unwrap();
        assert!(solver.generalize_from(&[source], &[]).unwrap().is_empty());
        assert_eq!(
            solver
                .resolve(&Row::variable(SetKind::Error, source), &mut unify)
                .unwrap(),
            Row::empty(SetKind::Error)
        );
    }
}
