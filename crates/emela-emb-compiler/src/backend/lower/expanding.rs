//! Find generic enum cycles that add constructors around a type parameter.
//!
//! Edges connect declaration parameters, not just enum names: a parameter discarded by an
//! intervening declaration cannot grow on the next cycle. Permutations remain finite.
use super::*;

pub(super) fn classify(program: &Program) -> HashSet<EnumId> {
    type Node = (EnumId, usize);
    let mut edges: HashMap<Node, Vec<(Node, bool)>> = HashMap::new();
    for (source, definition) in program.core.enums.iter() {
        let mut pending: Vec<_> = definition
            .variants
            .iter()
            .flat_map(|v| v.tys.iter().copied())
            .collect();
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match program.core.ty(ty) {
                Type::Enum(target, args) => {
                    for (position, &argument) in args.iter().enumerate() {
                        let mut parts = vec![(argument, false)];
                        let mut visited = HashSet::new();
                        while let Some((part, nested)) = parts.pop() {
                            if !visited.insert((part, nested)) {
                                continue;
                            }
                            match program.core.ty(part) {
                                Type::Param(index) => edges
                                    .entry((source, *index))
                                    .or_default()
                                    .push(((*target, position), nested)),
                                Type::Tuple(fields) | Type::Enum(_, fields) => {
                                    parts.extend(fields.iter().map(|&ty| (ty, true)))
                                }
                                Type::List(element) | Type::Cell(element) => {
                                    parts.push((*element, true))
                                }
                                Type::Fn(signature) => parts.extend(
                                    signature
                                        .params
                                        .iter()
                                        .copied()
                                        .chain([signature.ret])
                                        .map(|ty| (ty, true)),
                                ),
                                _ => {}
                            }
                        }
                    }
                    pending.extend(args);
                }
                Type::Tuple(fields) => pending.extend(fields),
                Type::List(element) | Type::Cell(element) => pending.push(*element),
                Type::Fn(signature) => {
                    pending.extend(signature.params.iter().copied().chain([signature.ret]))
                }
                _ => {}
            }
        }
    }
    let mut reverse: HashMap<Node, Vec<(Node, bool)>> = HashMap::new();
    for (&from, targets) in &edges {
        for &(to, grows) in targets {
            reverse.entry(to).or_default().push((from, grows));
        }
    }
    fn reachable(start: Node, edges: &HashMap<Node, Vec<(Node, bool)>>) -> HashSet<Node> {
        let mut found = HashSet::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            if found.insert(node) {
                pending.extend(
                    edges
                        .get(&node)
                        .into_iter()
                        .flatten()
                        .map(|&(target, _)| target),
                );
            }
        }
        found
    }
    let mut growing = HashSet::new();
    for (&from, targets) in &edges {
        for &(to, increases) in targets {
            if !increases || growing.contains(&from) {
                continue;
            }
            let after = reachable(to, &edges);
            if !after.contains(&from) {
                continue;
            }
            let before = reachable(from, &reverse);
            growing.extend(after.intersection(&before).copied());
        }
    }
    growing.into_iter().map(|(id, _)| id).collect()
}
