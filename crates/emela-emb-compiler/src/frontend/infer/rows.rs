//! Translate source type identities to the independent set solver.
use super::*;
use crate::frontend::sets::{self, Origin, Row, SetKind, Tag, VarId};

#[derive(Clone, Debug)]
pub(super) struct SourceTag {
    pub kind: SetKind,
    pub nominal: usize,
    pub args: Vec<TypeId>,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Rows {
    pub solver: sets::Solver,
    pub variables: HashMap<TypeId, (SetKind, VarId)>,
    pub proxies: HashMap<VarId, TypeId>,
    pub effects: HashMap<crate::contract::ir::EffectId, (usize, Vec<TypeId>, bool)>,
    pub tags: Vec<SourceTag>,
    tag_heads: HashMap<(bool, usize, Vec<TypeId>), usize>,
}

impl Rows {
    pub fn tag(&mut self, kind: SetKind, nominal: usize, args: Vec<TypeId>) -> Tag {
        let key = (kind == SetKind::Effect, nominal, args.clone());
        let head = *self.tag_heads.entry(key).or_insert_with(|| {
            let head = self.tags.len();
            self.tags.push(SourceTag {
                kind,
                nominal,
                args: args.clone(),
            });
            head
        });
        Tag { head, args }
    }
    pub fn variable(&mut self, proxy: TypeId, kind: SetKind, origin: Origin) -> Result<VarId> {
        if let Some(&(previous, variable)) = self.variables.get(&proxy) {
            if previous != kind {
                return Err(error(
                    TypeErrorKind::Constraint,
                    proxy,
                    proxy,
                    "a type parameter cannot be both an error set and an effect set",
                ));
            }
            return Ok(variable);
        }
        let variable = self.solver.fresh(kind, origin);
        self.variables.insert(proxy, (kind, variable));
        self.proxies.insert(variable, proxy);
        Ok(variable)
    }

    pub fn row(
        &mut self,
        types: &TypeTable,
        ty: TypeId,
        kind: SetKind,
        metas: &HashMap<TypeId, Meta>,
    ) -> Result<Row> {
        let mut row = Row::empty(kind);
        let mut pending = vec![ty];
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if let Some(bound) = metas.get(&ty).and_then(|meta| meta.binding) {
                pending.push(bound);
                continue;
            }
            match Solver::node(types, ty)? {
                Type::Set(summary) => pending.extend(summary.params),
                Type::Param(_) => {
                    let origin = if metas.contains_key(&ty) {
                        Origin::Inferred
                    } else {
                        Origin::OpaqueParameter
                    };
                    let variable = self.variable(ty, kind, origin)?;
                    if !row.vars.contains(&variable) {
                        row.vars.push(variable);
                    }
                }
                Type::Enum(id, args) if kind == SetKind::Error => {
                    row.tags
                        .push(self.tag(kind, id.into_raw().into_u32() as usize, args))
                }
                Type::Handler(id) if kind == SetKind::Effect => {
                    let (head, args, _) = self.effects.get(&id).cloned().unwrap_or((
                        id.into_raw().into_u32() as usize,
                        vec![],
                        false,
                    ));
                    row.tags.push(self.tag(kind, head, args));
                }
                _ => {
                    return Err(error(
                        TypeErrorKind::Constraint,
                        ty,
                        ty,
                        match kind {
                            SetKind::Error => {
                                "an error set requires error types or error-set parameters"
                            }
                            SetKind::Effect => {
                                "an effect set requires effects or effect-set parameters"
                            }
                        },
                    ));
                }
            }
        }
        Ok(row)
    }

    pub fn solve(&mut self) -> Result<Vec<(TypeId, TypeId)>> {
        let mut pairs = Vec::new();
        let effects = &self.effects;
        let tags = &self.tags;
        self.solver
            .solve(
                &mut |left, right| {
                    pairs.push((left, right));
                    Ok(())
                },
                &mut |tag| {
                    effects
                        .values()
                        .any(|(head, _, immediate)| *head == tags[tag.head].nominal && *immediate)
                },
            )
            .map_err(set_error)?;
        Ok(pairs)
    }

    pub fn resolved_type(&self, types: &mut TypeTable, proxy: TypeId) -> Result<TypeId> {
        let &(kind, variable) = self.variables.get(&proxy).ok_or_else(|| {
            error(
                TypeErrorKind::InvalidType,
                proxy,
                proxy,
                "unknown source set variable",
            )
        })?;
        let mut pairs = Vec::new();
        let row = self
            .solver
            .resolve(&Row::variable(kind, variable), &mut |left, right| {
                pairs.push((left, right));
                Ok(())
            })
            .map_err(set_error)?;
        let mut members = row
            .vars
            .into_iter()
            .filter_map(|variable| self.proxies.get(&variable).copied())
            .collect::<Vec<_>>();
        for tag in row.tags {
            let source = &self.tags[tag.head];
            let member = match kind {
                SetKind::Error => types.intern(Type::Enum(
                    la_arena::Idx::from_raw(la_arena::RawIdx::from_u32(source.nominal as u32)),
                    tag.args,
                )),
                SetKind::Effect => {
                    let id = self
                        .effects
                        .iter()
                        .find_map(|(&id, (head, args, _))| {
                            (*head == source.nominal && *args == tag.args).then_some(id)
                        })
                        .unwrap_or_else(|| {
                            la_arena::Idx::from_raw(la_arena::RawIdx::from_u32(
                                source.nominal as u32,
                            ))
                        });
                    types.intern(Type::Handler(id))
                }
            };
            members.push(member);
        }
        members.sort();
        members.dedup();
        Ok(types.intern(Type::Set(crate::contract::ir::SetSummary {
            params: members,
            ..Default::default()
        })))
    }
}

pub(super) fn set_error(failure: sets::SetError) -> TypeError {
    error(
        TypeErrorKind::Constraint,
        TypeId::UNIT,
        TypeId::UNIT,
        format!("function invocation set constraint failed: {failure:?}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn set(types: &mut TypeTable, members: Vec<TypeId>) -> TypeId {
        types.intern(Type::Set(crate::contract::ir::SetSummary {
            params: members,
            ..Default::default()
        }))
    }
    #[test]
    fn concrete_generic_errors_stay_distinct_while_unknown_arguments_are_inferred() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let declaration = la_arena::Idx::from_raw(la_arena::RawIdx::from_u32(0));
        let integer = types.intern(Type::Enum(declaration, vec![TypeId::INT]));
        let string = types.intern(Type::Enum(declaration, vec![TypeId::STRING]));
        let destination = solver
            .fresh_row(&mut types, SetKind::Error, Origin::Inferred)
            .unwrap();
        let both = set(&mut types, vec![integer, string]);
        solver
            .include_sets(&mut types, both, destination, SetKind::Error)
            .unwrap();
        let resolved = solver.resolve_row(&mut types, destination).unwrap();
        let Type::Set(row) = &types[resolved] else {
            panic!("resolved row")
        };
        assert_eq!(row.params.len(), 2);
        let meta = solver.try_fresh(&mut types, Constraint::Any).unwrap();
        let unknown = types.intern(Type::Enum(declaration, vec![meta]));
        let actual = set(&mut types, vec![unknown]);
        let expected = set(&mut types, vec![integer]);
        solver
            .include_sets(&mut types, actual, expected, SetKind::Error)
            .unwrap();
        assert_eq!(solver.resolve(&mut types, meta).unwrap(), TypeId::INT);
    }
    #[test]
    fn a_row_parameter_cannot_mix_error_and_effect_kinds() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let row = solver
            .fresh_row(&mut types, SetKind::Error, Origin::HiddenParameter)
            .unwrap();
        assert!(
            solver
                .register_row(row, SetKind::Effect, Origin::HiddenParameter)
                .is_err()
        );
    }
    #[test]
    fn generic_effect_instances_keep_their_value_arguments() {
        let mut core = crate::contract::ir::Definitions::default();
        let mut solver = Solver::new();
        let effects = [TypeId::INT, TypeId::STRING]
            .into_iter()
            .map(|argument| {
                let id = core.effects.alloc(crate::contract::ir::EffectDef {
                    name: "State".into(),
                    decl: "State".into(),
                    args: vec![argument],
                    ops: vec![],
                    label_of: None,
                });
                core.types.intern(Type::Handler(id))
            })
            .collect();
        solver.register_effects(&core);
        let row = solver
            .fresh_row(&mut core.types, SetKind::Effect, Origin::Inferred)
            .unwrap();
        let actual = set(&mut core.types, effects);
        solver
            .include_sets(&mut core.types, actual, row, SetKind::Effect)
            .unwrap();
        let resolved = solver.resolve_row(&mut core.types, row).unwrap();
        let Type::Set(row) = &core.types[resolved] else {
            panic!("resolved effect row")
        };
        assert_eq!(row.params.len(), 2);
    }
    #[test]
    fn function_parameters_are_contravariant_and_named_type_arguments_invariant() {
        let mut types = TypeTable::default();
        let mut solver = Solver::new();
        let empty = set(&mut types, vec![]);
        let declaration = la_arena::Idx::from_raw(la_arena::RawIdx::from_u32(0));
        let error = types.intern(Type::Enum(declaration, vec![]));
        let errors = set(&mut types, vec![error]);
        let function = |types: &mut TypeTable, params, errors| {
            types.intern(Type::Fn(crate::contract::ir::FnType {
                params,
                ret: TypeId::INT,
                fails: false,
                suspends: false,
                ev: false,
                sets: Some(FnSets {
                    errors,
                    effects: empty,
                }),
            }))
        };
        let pure = function(&mut types, vec![], empty);
        let fallible = function(&mut types, vec![], errors);
        assert!(solver.unify(&mut types, pure, fallible).is_ok());
        assert!(solver.unify(&mut types, fallible, pure).is_err());
        let accepts_pure = function(&mut types, vec![pure], empty);
        let accepts_fallible = function(&mut types, vec![fallible], empty);
        assert!(
            solver
                .unify(&mut types, accepts_fallible, accepts_pure)
                .is_ok()
        );
        assert!(
            solver
                .unify(&mut types, accepts_pure, accepts_fallible)
                .is_err()
        );
        let holder_pure = types.intern(Type::Enum(declaration, vec![pure]));
        let holder_fallible = types.intern(Type::Enum(declaration, vec![fallible]));
        assert!(
            solver
                .unify(&mut types, holder_pure, holder_fallible)
                .is_err()
        );
    }
}
