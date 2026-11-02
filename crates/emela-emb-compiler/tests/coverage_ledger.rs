//! Guard the migration ledger's accounting and evidence references. These checks do not
//! establish semantic parity: completing a row still requires the documented human review.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join(path)).unwrap()).unwrap()
}

fn rows(value: &Value) -> &[Value] {
    value.as_array().expect("ledger field must be an array")
}

fn key(row: &Value, fields: &[&str]) -> Vec<String> {
    fields
        .iter()
        .map(|field| row[field].as_str().unwrap().to_owned())
        .collect()
}

fn indexed<'a>(values: &'a [Value], fields: &[&str]) -> BTreeMap<Vec<String>, &'a Value> {
    let mut map = BTreeMap::new();
    for row in values {
        let id = key(row, fields);
        assert!(map.insert(id.clone(), row).is_none(), "duplicate {id:?}");
    }
    map
}

#[test]
fn migration_ids_match_the_fixed_reference_without_missing_or_extra_rows() {
    let ledger = read("docs/compiler-coverage.json");
    let backend = read("docs/reference/legacy-flow/flow-backend-inventory.json");
    let coverage = read("docs/reference/legacy-flow/flow-coverage.json");
    let provenance = read("docs/reference/legacy-flow/provenance.json");
    assert_eq!(ledger["baseline_revision"], provenance["source_revision"]);
    let records = indexed(rows(&ledger["records"]), &["id"]);
    let reference = indexed(rows(&backend["records"]), &["id"]);
    assert_eq!(
        records.keys().collect::<Vec<_>>(),
        reference.keys().collect::<Vec<_>>()
    );
    assert_eq!(records.len(), 428);
    for (id, row) in &records {
        assert_eq!(row["relation"], reference[id]["relation"], "{id:?}");
        assert_eq!(row["legacy_role"], reference[id]["role"], "{id:?}");
    }
    let contexts = indexed(rows(&ledger["contexts"]), &["id", "context"]);
    let expected_contexts: BTreeSet<_> = rows(&coverage["items"])
        .iter()
        .flat_map(|row| {
            rows(&row["contexts"]).iter().map(move |context| {
                vec![
                    row["id"].as_str().unwrap().to_owned(),
                    context["context"].as_str().unwrap().to_owned(),
                ]
            })
        })
        .collect();
    assert_eq!(
        contexts.keys().cloned().collect::<BTreeSet<_>>(),
        expected_contexts
    );
    assert_eq!(contexts.len(), 226);
    let tests = indexed(rows(&ledger["tests"]), &["target", "name"]);
    let mut expected_tests = BTreeMap::new();
    for record in rows(&backend["records"]) {
        for test in rows(&record["tests"]) {
            let old = expected_tests.insert(key(test, &["target", "name"]), &test["path"]);
            if let Some(path) = old {
                assert_eq!(path, &test["path"]);
            }
        }
    }
    assert_eq!(
        tests.keys().collect::<Vec<_>>(),
        expected_tests.keys().collect::<Vec<_>>()
    );
    assert_eq!(tests.len(), 115);
    for (id, row) in tests {
        assert_eq!(&row["legacy_path"], expected_tests[&id], "{id:?}");
    }
    let requirements = indexed(rows(&ledger["requirements"]), &["id"]);
    let expected: BTreeSet<_> = [
        "source_corpus",
        "frontend_validation",
        "standard_library",
        "memory_ownership",
        "const_identity",
        "concurrency_cleanup",
        "c_abi_isr_services",
        "numeric_unicode",
        "standalone_execution",
        "known_differences",
    ]
    .into_iter()
    .map(|id| vec![id.to_owned()])
    .collect();
    assert_eq!(
        requirements.keys().cloned().collect::<BTreeSet<_>>(),
        expected
    );
}

fn check_references(row: &Value, field: &str) {
    for reference in rows(&row[field]) {
        assert!(!reference["kind"].as_str().unwrap().trim().is_empty());
        let path = reference["path"].as_str().unwrap();
        assert!(
            !path.is_empty() && !path.contains('\\'),
            "invalid evidence {path}"
        );
        assert!(
            Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "non-relative evidence {path}"
        );
        let target = root().join(path);
        assert!(target.is_file(), "missing evidence {path}");
        assert!(
            target
                .canonicalize()
                .unwrap()
                .starts_with(root().canonicalize().unwrap()),
            "evidence escapes repository: {path}"
        );
        if let Some(symbol) = reference.get("symbol") {
            assert!(!symbol.as_str().unwrap().trim().is_empty());
        }
    }
}

#[test]
fn final_states_require_local_evidence_and_cannot_silently_drop_source_operations() {
    let ledger = read("docs/compiler-coverage.json");
    for section in ["records", "contexts", "tests", "requirements"] {
        for row in rows(&ledger[section]) {
            let status = row["status"].as_str().unwrap();
            let allowed: &[&str] = match section {
                "requirements" => &["pending", "mapped", "verified"],
                "tests" => &[
                    "pending",
                    "mapped",
                    "verified",
                    "replaced",
                    "not_applicable",
                ],
                _ => &["pending", "mapped", "verified", "not_applicable"],
            };
            assert!(
                allowed.contains(&status),
                "unknown {section} status {status}"
            );
            check_references(row, "evidence");
            if section == "records" {
                check_references(row, "new_implementation");
            } else if section == "tests" {
                check_references(row, "new_tests");
            }
            if let Some(id) = row["id"].as_str() {
                if let Some(builtin) = id.strip_prefix("emela_core::Builtin::") {
                    if [
                        "AssertFail",
                        "Dbg",
                        "DebugPrint",
                        "Inspect",
                        "Panic",
                        "ShowNested",
                        "ShowString",
                        "Todo",
                    ]
                    .contains(&builtin)
                    {
                        assert_ne!(
                            status, "not_applicable",
                            "source operation {id} cannot be excluded"
                        );
                    }
                }
            }
            if ["pending", "mapped"].contains(&status) {
                continue;
            }
            assert!(
                !rows(&row["evidence"]).is_empty(),
                "final row lacks evidence: {row}"
            );
            if matches!(status, "not_applicable" | "replaced") {
                assert!(
                    !row["reason"].as_str().unwrap().trim().is_empty(),
                    "final exception lacks reason: {row}"
                );
            }
            if section == "records" {
                assert!(
                    ["P1", "P2", "P3", "P4", "P5", "P6", "P7"]
                        .contains(&row["phase"].as_str().unwrap())
                );
                assert!(
                    [
                        "source",
                        "ir_only",
                        "input_only",
                        "schema",
                        "not_applicable"
                    ]
                    .contains(&row["source_scope"].as_str().unwrap())
                );
                if status == "verified" {
                    assert!(!rows(&row["new_implementation"]).is_empty());
                }
            } else if section == "tests" {
                assert!(!rows(&row["new_tests"]).is_empty());
            }
            if matches!(status, "verified" | "replaced") {
                assert!(
                    rows(&row["evidence"])
                        .iter()
                        .any(|r| r["kind"] == "run_report"),
                    "verified row lacks run report: {row}"
                );
            }
        }
    }
}
