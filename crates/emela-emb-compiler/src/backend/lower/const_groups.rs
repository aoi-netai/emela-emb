//! Explicit source const families replace the legacy unreachable origin marker functions.
use super::*;
use crate::contract::lowered::ConstGroup;

pub(super) fn prepare(program: &mut Program) -> Result<Vec<ConstGroup>> {
    let mut families: HashMap<FnId, Vec<FnId>> = HashMap::new();
    for (id, function) in program.functions.iter() {
        let Some(canonical) = function.canonical_const else {
            continue;
        };
        if !function.constant
            || !function.params.is_empty()
            || function.fails
            || function.suspends
            || function.external.is_some()
            || function.exported
        {
            return Err(invalid(&function.name, "invalid shared const member ABI"));
        }
        let canonical_function = program
            .functions
            .iter()
            .find_map(|(id, f)| (id == canonical).then_some(f))
            .ok_or_else(|| invalid(&function.name, "canonical const ID is out of bounds"))?;
        if canonical_function.canonical_const != Some(canonical) || !canonical_function.constant {
            return Err(invalid(
                &function.name,
                "canonical const does not identify its own family",
            ));
        }
        families.entry(canonical).or_default().push(id);
    }
    let mut groups = Vec::new();
    for (canonical, members) in families {
        let specializations = members
            .into_iter()
            .filter(|&id| id != canonical)
            .collect::<Vec<_>>();
        if specializations.is_empty() {
            return Err(invalid(
                &program.functions[canonical].name,
                "shared const family has no typed member",
            ));
        }
        groups.push(ConstGroup {
            canonical,
            specializations,
        });
    }
    groups.sort_by_key(|group| group.canonical.into_raw());
    for group in &groups {
        for &id in &group.specializations {
            let function = &mut program.functions[id];
            function.constant = false;
            let edge = function.stmts.alloc(Stmt::Let {
                dst: None,
                value: Value::Call {
                    callee: Callee::Direct(group.canonical),
                    args: vec![],
                    suspends: false,
                    on_fail: None,
                },
                on_defect: None,
                on_suspend: None,
            });
            let body = function
                .body
                .ok_or_else(|| invalid(&function.name, "shared const has no initializer body"))?;
            // The emitter replaces this getter. This edge retains the canonical body and
            // both callable families until bridge planning has paired their environments.
            function.seqs[body].stmts.insert(0, edge);
        }
    }
    Ok(groups)
}
