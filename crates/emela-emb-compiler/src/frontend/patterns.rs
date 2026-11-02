//! Constructor-matrix coverage, shared by match exhaustiveness and irrefutable bindings.
use super::{
    data::{Catalog, backend_type, frontend_type},
    ir::{Literal, Pattern, PatternKind as P},
    types::ValueType as Ty,
};
use crate::contract::ir::{CtorRef, Type};
#[derive(Clone, Copy, PartialEq)]
enum Head {
    Bool(bool),
    Unit,
    Tuple,
    ListNil,
    ListCons,
    Variant(CtorRef),
}
pub(crate) fn exhaustive(patterns: &[Pattern], ty: Ty, catalog: &mut Catalog) -> bool {
    matrix(
        patterns.iter().cloned().map(|p| vec![p]).collect(),
        vec![ty],
        catalog,
        0,
    )
}
fn any(pattern: &Pattern) -> bool {
    match &pattern.kind {
        P::Constant {
            coverage: Some(coverage),
            ..
        } => any(coverage),
        _ => matches!(pattern.kind, P::Any | P::Bind(_)),
    }
}
fn head(pattern: &Pattern) -> Option<(Head, Vec<Pattern>)> {
    match &pattern.kind {
        P::Constant {
            coverage: Some(coverage),
            ..
        } => head(coverage),
        P::Literal(Literal::Bool(value)) => Some((Head::Bool(*value), Vec::new())),
        P::Literal(Literal::Unit) => Some((Head::Unit, Vec::new())),
        P::Tuple(fields) => Some((Head::Tuple, fields.clone())),
        P::ListNil => Some((Head::ListNil, vec![])),
        P::ListCons { head, tail } => Some((Head::ListCons, vec![*head.clone(), *tail.clone()])),
        P::Constructor { ctor, fields } => Some((Head::Variant(*ctor), fields.clone())),
        _ => None,
    }
}
fn constructors(ty: Ty, catalog: &mut Catalog) -> Option<Vec<(Head, Vec<Ty>)>> {
    Some(match catalog.core.ty(backend_type(ty)).clone() {
        Type::Bool => vec![(Head::Bool(false), vec![]), (Head::Bool(true), vec![])],
        Type::Unit => vec![(Head::Unit, vec![])],
        Type::List(element) => vec![
            (Head::ListNil, vec![]),
            (Head::ListCons, vec![frontend_type(element), ty]),
        ],
        Type::Tuple(fields) => vec![(Head::Tuple, fields.into_iter().map(frontend_type).collect())],
        Type::Enum(id, _) => {
            let count = catalog.core.enums[id].variants.len();
            if count == 0 {
                return None;
            }
            (0..count)
                .map(|variant| {
                    let ctor = CtorRef {
                        enum_id: id,
                        variant,
                    };
                    (Head::Variant(ctor), catalog.fields(ctor, ty))
                })
                .collect()
        }
        _ => return None,
    })
}
fn matrix(rows: Vec<Vec<Pattern>>, types: Vec<Ty>, catalog: &mut Catalog, depth: usize) -> bool {
    if rows.is_empty() {
        return false;
    }
    if types.is_empty() || rows.iter().any(|row| row.iter().all(any)) {
        return true;
    }
    if depth >= 512 {
        return false;
    }
    if let Some(constructors) = constructors(types[0], catalog) {
        for (constructor, fields) in constructors {
            let mut specialized = Vec::new();
            for row in &rows {
                let mut values = if any(&row[0]) {
                    fields
                        .iter()
                        .map(|&ty| Pattern {
                            kind: P::Any,
                            ty,
                            span: row[0].span,
                        })
                        .collect::<Vec<_>>()
                } else if let Some((kind, values)) = head(&row[0]) {
                    if kind == constructor {
                        values
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };
                values.extend_from_slice(&row[1..]);
                specialized.push(values);
            }
            let mut remaining = fields;
            remaining.extend_from_slice(&types[1..]);
            if !matrix(specialized, remaining, catalog, depth + 1) {
                return false;
            }
        }
        true
    } else {
        let defaults = rows
            .into_iter()
            .filter(|row| any(&row[0]))
            .map(|row| row[1..].to_vec())
            .collect();
        matrix(defaults, types[1..].to_vec(), catalog, depth + 1)
    }
}
