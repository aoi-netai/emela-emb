#!/usr/bin/env python3
"""Cross-reference completion obligations without changing the authoritative ledger.

Inputs are committed objects from this repository and the saved legacy index.
Reference existence and test discovery are structural observations, never a
substitute for a semantic review or a C execution result.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

sys.dont_write_bytecode = True
scanner_spec = importlib.util.spec_from_file_location("legacy_index_scanner", Path(__file__).with_name("index_legacy_corpus.py"))
scanner = importlib.util.module_from_spec(scanner_spec)
sys.modules[scanner_spec.name] = scanner
scanner_spec.loader.exec_module(scanner)


class Snapshot:
    def __init__(self, revision: str):
        self.revision = self.git("rev-parse", f"{revision}^{{commit}}").strip()
        self.tree = {}
        for entry in self.git("ls-tree", "-r", "-z", self.revision).split("\0"):
            if not entry:
                continue
            metadata, path = entry.split("\t", 1)
            _, kind, oid = metadata.split()
            if kind == "blob":
                self.tree[path] = oid
        self.cache = {}

    @staticmethod
    def git(*args: str) -> str:
        return subprocess.check_output(["git", *args], text=True)

    def read(self, path: str) -> str:
        if path not in self.cache:
            self.cache[path] = self.git("show", f"{self.revision}:{path}")
        return self.cache[path]


def digest(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def test_record(function, revision: str) -> dict:
    body = function.file.raw(function.body_start, function.end)
    tokens = function.file.tokens[function.body_start:function.end]
    names = {token.text for token in tokens if token.kind == "identifier"}
    return {
        "revision": revision,
        "path": function.file.path,
        "symbol": function.qualified_name,
        "leaf_name": function.name,
        "line": function.file.line(function.file.tokens[function.start].start),
        "body_sha256": digest(body),
        "attributes": function.inherited + function.attributes,
        "role": "source-integration-test" if "/tests/" in function.file.path else "backend-IR-unit-test" if "/backend/" in function.file.path else "frontend-unit-test",
        "direct_C_helper_seen": sorted(names & {"compile_and_run", "run_c"}),
        "execution_inferred_from_discovery": False,
    }


def matches(group: dict, collection: str, key: str) -> bool:
    if collection in ("records", "contexts"):
        return any(key == p or key.startswith(p + ".") for p in group.get("record_ids", [])) or any(key == p or key.startswith(p + ".") or key.startswith(p + "::") for p in group.get("record_prefixes", []))
    if collection == "tests":
        return key in group.get("test_ids", []) or any(key.startswith(p) for p in group.get("test_prefixes", []))
    return key in group.get("requirement_ids", [])


def check_fixed_sets(ledger: dict, base: Snapshot) -> dict:
    backend = json.loads(base.read("docs/reference/legacy-flow/flow-backend-inventory.json"))
    coverage = json.loads(base.read("docs/reference/legacy-flow/flow-coverage.json"))
    provenance = json.loads(base.read("docs/reference/legacy-flow/provenance.json"))
    if ledger["baseline_revision"] != provenance["source_revision"]:
        raise ValueError("legacy baseline differs from saved provenance")
    fixed_records = {row["id"]: row for row in backend["records"]}
    if set(fixed_records) != {row["id"] for row in ledger["records"]}:
        raise ValueError("record IDs differ from fixed reference")
    for row in ledger["records"]:
        fixed = fixed_records[row["id"]]
        if (row["relation"], row["legacy_role"]) != (fixed["relation"], fixed["role"]):
            raise ValueError(f"legacy record metadata differs: {row['id']}")
    fixed_contexts = {(row["id"], context["context"]) for row in coverage["items"] for context in row["contexts"]}
    if fixed_contexts != {(row["id"], row["context"]) for row in ledger["contexts"]}:
        raise ValueError("context IDs differ from fixed reference")
    fixed_tests = {}
    for row in backend["records"]:
        for test in row["tests"]:
            key = test["target"], test["name"]
            if key in fixed_tests and fixed_tests[key] != test["path"]:
                raise ValueError(f"inconsistent fixed test path: {key}")
            fixed_tests[key] = test["path"]
    if fixed_tests != {(row["target"], row["name"]): row["legacy_path"] for row in ledger["tests"]}:
        raise ValueError("test IDs/paths differ from fixed reference")
    required = {"source_corpus", "frontend_validation", "standard_library", "memory_ownership", "const_identity", "concurrency_cleanup", "c_abi_isr_services", "numeric_unicode", "standalone_execution", "known_differences"}
    if required != {row["id"] for row in ledger["requirements"]}:
        raise ValueError("completion requirements differ")
    return {"record_ids_roles_relations": "match", "context_ids": "match", "test_ids_paths": "match", "requirement_ids": "match"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--reviews", type=Path, default=Path("docs/tools/legacy_completion_reviews.json"))
    parser.add_argument("--corpus", type=Path, default=Path("docs/compiler-legacy-source-corpus.json"))
    parser.add_argument("--out", type=Path, default=Path("docs/compiler-p7-completion-audit.json"))
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    base = Snapshot(args.revision)
    ledger_text = base.read("docs/compiler-coverage.json")
    ledger = json.loads(ledger_text)
    fixed_sets = check_fixed_sets(ledger, base)
    reviews_text = args.reviews.read_text()
    reviews = json.loads(reviews_text)
    corpus_text = args.corpus.read_text()
    corpus = {test["id"]: test for test in json.loads(corpus_text)["tests"]}
    snapshots = {base.revision: base}

    def snapshot(revision: str | None) -> Snapshot:
        if revision is None or revision == "baseline":
            return base
        if revision not in snapshots:
            snapshots[revision] = Snapshot(revision)
        return snapshots[revision]

    scanner.CRATE = "crates/emela-emb-compiler"
    inventory = scanner.Inventory(base)
    inventory.discover()
    tests = [test_record(f, base.revision) for f in inventory.functions.values() if f.test]
    if inventory.unresolved:
        raise ValueError(f"unresolved new test discovery: {inventory.unresolved}")
    supplemental_files = set()
    for group in reviews["groups"]:
        for ref in group.get("candidate_evidence", []):
            if ref.get("revision", "baseline") != "baseline" and ref["path"].endswith(".rs"):
                supplemental_files.add((ref["revision"], ref["path"]))
    for revision, path in sorted(supplemental_files):
        objects = snapshot(revision)
        inv = scanner.Inventory(objects)
        file = inv.rust(path)
        inv.region(file, str(Path(path).parent), "supplement", (), [], 0, len(file.tokens))
        tests.extend(test_record(f, objects.revision) for f in inv.functions.values() if f.test)
    tests.sort(key=lambda test: (test["revision"], test["path"], test["line"]))
    by_name = defaultdict(list)
    for test in tests:
        by_name[test["leaf_name"]].append(test)

    references = {}

    def reference(ref: dict) -> str:
        objects = snapshot(ref.get("revision"))
        key = f"{objects.revision}:{ref['path']}" + (f"#{ref['symbol']}" if ref.get("symbol") else "")
        if key not in references:
            exists = ref["path"] in objects.tree
            symbol = ref.get("symbol")
            text = objects.read(ref["path"]) if exists and symbol else ""
            references[key] = {
                **ref,
                "roles": [ref["kind"]],
                "revision": objects.revision,
                "path_exists": exists,
                "blob": objects.tree.get(ref["path"]),
                "symbol_components_seen": all(re.search(r"\b" + re.escape(component) + r"\b", text) for component in symbol.split("::")) if symbol and exists else None,
                "semantic_correspondence_verified": False,
            }
        elif ref["kind"] not in references[key]["roles"]:
            references[key]["roles"].append(ref["kind"])
            references[key]["roles"].sort()
        return key

    groups = []
    for group in reviews["groups"]:
        groups.append({**group, "candidate_reference_ids": [reference(ref) for ref in group.get("candidate_evidence", [])]})
    output = {
        "schema_version": 1,
        "legacy_revision": ledger["baseline_revision"],
        "implementation_revision": base.revision,
        "ledger_sha256": digest(ledger_text),
        "review_sha256": digest(reviews_text),
        "corpus_index_sha256": digest(corpus_text),
        "purpose": "Read-only completion audit. The authoritative ledger and its statuses are not changed. Candidate matches need semantic review; C generation/object checks are not C execution.",
        "fixed_reference_checks": fixed_sets,
        "reviews": groups,
        "collections": {},
        "new_tests": tests,
    }
    final_statuses = {"records": {"verified", "not_applicable"}, "contexts": {"verified", "not_applicable"}, "tests": {"verified", "replaced", "not_applicable"}, "requirements": {"verified"}}
    for collection in ("records", "contexts", "tests", "requirements"):
        rows = []
        keys = set()
        for row in ledger[collection]:
            key = f"{row['target']}::{row['name']}" if collection == "tests" else row["id"]
            row_key = f"{key}@{row['context']}" if collection == "contexts" else key
            if row_key in keys:
                raise ValueError(f"duplicate ledger key: {collection}:{row_key}")
            keys.add(row_key)
            record = {k: row[k] for k in ("id", "target", "name", "context", "status", "phase", "source_scope", "relation", "legacy_role", "reason") if k in row}
            record["key"] = row_key
            record["ledger_final_status"] = row["status"] in final_statuses[collection]
            record["review_groups"] = [group["id"] for group in groups if matches(group, collection, key)]
            record["ledger_reference_ids"] = sorted({reference(ref) for column in ("new_implementation", "new_tests", "evidence") for ref in row.get(column, [])})
            obligations = []
            if not record["ledger_final_status"]:
                obligations.append("ledger-obligation-open")
            if collection == "records" and row["phase"] is None:
                obligations.append("phase-unassigned")
            if collection == "records" and row["source_scope"] == "unclassified":
                obligations.append("source-scope-unclassified")
            if not record["review_groups"] and not record["ledger_final_status"]:
                obligations.append("no-manual-review-group")
            if collection == "tests":
                old = corpus.get(key)
                record["legacy_index"] = {k: old[k] for k in ("id", "path", "line", "category", "ignored")} if old else None
                record["exact_new_name_candidates"] = by_name[row["name"].split("::")[-1]]
                if old is None:
                    obligations.append("legacy-test-not-in-corpus-index")
                if old and old["category"] == "source" and not record["ledger_final_status"]:
                    obligations.append("legacy-source-observations-need-source-and-C-mapping")
            record["open_obligations"] = obligations
            rows.append(record)
        output["collections"][collection] = {
            "count": len(rows),
            "ledger_status_counts": dict(sorted(Counter(row["status"] for row in rows).items())),
            "open_obligation_counts": dict(sorted(Counter(o for row in rows for o in row["open_obligations"]).items())),
            "rows": rows,
        }
    output["references"] = dict(sorted(references.items()))
    output["reference_checks"] = {
        "count": len(references),
        "missing_paths": [key for key, value in sorted(references.items()) if not value["path_exists"]],
        "symbol_components_not_seen": [key for key, value in sorted(references.items()) if value["symbol_components_seen"] is False],
        "warning": "Text presence checks do not validate a symbol's meaning or an executed test.",
    }
    generated = json.dumps(output, ensure_ascii=False, indent=2) + "\n"
    if args.check:
        if args.out.read_text() != generated:
            raise ValueError(f"generated completion audit differs: {args.out}")
    else:
        args.out.write_text(generated)
    print(json.dumps({k: {key: value for key, value in group.items() if key != "rows"} for k, group in output["collections"].items()}, ensure_ascii=False))


if __name__ == "__main__":
    main()
