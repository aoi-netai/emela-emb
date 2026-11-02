//! Generalize a complete recursive component before instantiating its callers.
use super::{data::backend_type, infer::Solver, ir::DefinitionId, types::Signature};
use crate::contract::ir::{Type, TypeId, TypeTable};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct Schemes {
    /// Explicit parameters come first; inferred rigid IDs are canonicalized by normalize.
    pub(crate) parameters: Vec<Vec<TypeId>>,
    nodes: Vec<Node>,
    calls: Vec<DefinitionId>,
    pending: Vec<DefinitionId>,
    next: usize,
    deferred: Vec<Deferred>,
}
struct Deferred {
    target: DefinitionId,
    template: TypeId,
    instance: TypeId,
    substitutions: HashMap<TypeId, TypeId>,
}
#[derive(Default)]
struct Node {
    index: usize,
    low: usize,
    open: bool,
}
impl Schemes {
    pub(crate) fn defer_return(
        &mut self,
        target: DefinitionId,
        template: TypeId,
        instance: TypeId,
        substitutions: HashMap<TypeId, TypeId>,
    ) {
        self.deferred.push(Deferred {
            target,
            template,
            instance,
            substitutions,
        });
    }
    pub(crate) fn initialize(&mut self, parameters: Vec<Vec<TypeId>>) {
        self.nodes = (0..parameters.len()).map(|_| Node::default()).collect();
        self.parameters = parameters;
    }
    pub(crate) fn begin(&mut self, id: DefinitionId) {
        self.nodes[id.0] = Node {
            index: self.next,
            low: self.next,
            open: true,
        };
        self.next += 1;
        self.calls.push(id);
        self.pending.push(id);
    }
    pub(crate) fn reference(&mut self, id: DefinitionId) {
        if let Some(caller) = self.calls.last().copied() {
            if self.nodes[id.0].open {
                self.nodes[caller.0].low = self.nodes[caller.0].low.min(self.nodes[id.0].index);
            }
        }
    }
    pub(crate) fn finish(&mut self, id: DefinitionId) -> Vec<DefinitionId> {
        let current = self.calls.pop();
        debug_assert_eq!(current, Some(id));
        let mut component = Vec::new();
        if self.nodes[id.0].low == self.nodes[id.0].index {
            loop {
                let member = self.pending.pop().expect("open recursive component");
                self.nodes[member.0].open = false;
                component.push(member);
                if member == id {
                    break;
                }
            }
        }
        if let Some(caller) = self.calls.last().copied() {
            self.nodes[caller.0].low = self.nodes[caller.0].low.min(self.nodes[id.0].low);
        }
        component
    }
    pub(crate) fn generalize(
        &mut self,
        component: &[DefinitionId],
        signatures: &[Signature],
        constants: &[bool],
        types: &mut TypeTable,
        solver: &mut Solver,
    ) -> Result<(), super::infer::TypeError> {
        // A recursive call can precede inference of its callee's result. Substitute
        // explicit parameters after the body supplies that result; sharing the raw
        // result meta would confuse Param(0) in two different function signatures.
        let mut deferred = Vec::new();
        let mut retained = Vec::new();
        for call in std::mem::take(&mut self.deferred) {
            if component.contains(&call.target) {
                deferred.push(call);
            } else {
                retained.push(call);
            }
        }
        self.deferred = retained;
        while !deferred.is_empty() {
            let mut selected = 0;
            for (index, call) in deferred.iter().enumerate() {
                let resolved = solver.resolve(types, call.template)?;
                if !solver.is_meta(resolved) {
                    selected = index;
                    break;
                }
            }
            let call = deferred.remove(selected);
            let actual = solver.substitute(types, call.template, &call.substitutions)?;
            solver.unify(types, call.instance, actual)?;
        }
        let mut environment = HashSet::new();
        let mut values = Vec::new();
        for (index, signature) in signatures.iter().enumerate() {
            let signature_values = signature_types(signature);
            if component.contains(&DefinitionId(index)) && !constants[index] {
                values.extend(signature_values);
            } else {
                // Monomorphic constants and unfinished surrounding functions keep their
                // variables shared. A callee cannot quantify a variable owned by them.
                environment.extend(signature_values);
            }
        }
        let tuple = types.intern(Type::Tuple(values));
        let row_candidates = component
            .iter()
            .flat_map(|id| signatures[id.0].params.iter().copied().map(backend_type))
            .collect::<Vec<_>>();
        let scheme = solver.generalize_with_rows(types, tuple, &environment, &row_candidates)?;
        for &id in component {
            if constants[id.0] {
                continue;
            }
            let mut seen = HashSet::new();
            let mut pending = signature_types(&signatures[id.0]);
            while let Some(ty) = pending.pop() {
                let ty = solver.resolve(types, ty)?;
                if !seen.insert(ty) {
                    continue;
                }
                match &types[ty] {
                    Type::Tuple(fields) | Type::Enum(_, fields) => pending.extend(fields),
                    Type::List(element) | Type::Cell(element) => pending.push(*element),
                    Type::Fn(function) => {
                        pending.extend(&function.params);
                        pending.push(function.ret);
                        if let Some(sets) = &function.sets {
                            pending.extend([sets.errors, sets.effects]);
                        }
                    }
                    Type::Set(summary) => pending.extend(&summary.params),
                    _ => {}
                }
            }
            self.parameters[id.0].extend(scheme.params.iter().filter(|p| seen.contains(p)));
        }
        Ok(())
    }
}
fn signature_types(signature: &Signature) -> Vec<TypeId> {
    signature
        .params
        .iter()
        .copied()
        .chain(signature.ret)
        .map(backend_type)
        .chain(signature.sets.errors.iter().copied())
        .chain(signature.sets.error_constraints.iter().copied())
        .chain(signature.sets.effects.iter().copied())
        .collect()
}
