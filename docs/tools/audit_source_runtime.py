#!/usr/bin/env python3
"""List every normal legacy source case and its actual runtime evidence boundary."""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess


def digest(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


class Objects:
    def __init__(self):
        self.cache = {}

    def read(self, revision: str, path: str) -> str:
        key = revision, path
        if key not in self.cache:
            self.cache[key] = subprocess.check_output(["git", "show", f"{revision}:{path}"], text=True)
        return self.cache[key]


def category(result: dict) -> str | None:
    if result["status"] != "compiler-rejected":
        return None
    text = result.get("diagnostics", "")
    if "with_task_adapter" in text:
        return "probe-task-adapter-not-supplied"
    if "non-scalar signature" in text or "with_external_adapter" in text:
        return "probe-external-ABI-contract-needs-Options-review"
    if "generic functions require a concrete wrapper" in text:
        return "concrete-C-root-wrapper-required"
    if "frame" in text and any(word in text for word in ("capacity", "bytes", "fit")):
        return "probe-frame-configuration-needs-review"
    if "MissingTag" in text:
        return "frontend-row-inference-at-snapshot-fixed-after-1086f6a"
    if "Embedded." in text:
        return "P7-Embedded-checkpoint-not-in-snapshot"
    if "closure in a const function" in text or "cannot unify" in text:
        return "type-or-const-checkpoint-review-required"
    return "backend-or-source-diagnostic-review-required"


def reach_record(program: dict) -> dict:
    def result(value: dict) -> dict:
        return {
            key: value[key] for key in ("status", "frontend", "C_generation", "diagnostics", "runtime_verified") if key in value
        } | {"reviewed_issue_category": category(value), "C_objects": value.get("C_objects", {}).get("status")}
    return {"raw": result(program["raw"]), "concrete_roots": [{"root": root["root"], **result(root)} for root in program["public_root_probes"]]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, default=Path("crates/emela-emb-compiler/tests/compat/legacy-corpus/manifest.json"))
    parser.add_argument("--corpus", type=Path, default=Path("docs/compiler-legacy-source-corpus.json"))
    parser.add_argument("--reviews", type=Path, default=Path("docs/tools/legacy_source_runtime_reviews.json"))
    parser.add_argument("--reach", type=Path, default=Path("docs/compiler-p6-source-reach.json"))
    parser.add_argument("--out", type=Path, default=Path("docs/compiler-legacy-runtime-crosswalk.json"))
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    fixture_text = args.fixtures.read_text()
    manifest = json.loads(fixture_text)
    programs = {program["id"]: program for program in manifest["programs"]}
    corpus_document = json.loads(args.corpus.read_text())
    corpus = {test["id"]: test for test in corpus_document["tests"]}
    reviews = json.loads(args.reviews.read_text())
    reach_text = args.reach.read_text()
    reach = json.loads(reach_text)
    if reach["fixture_manifest_sha256"] != digest(fixture_text):
        raise ValueError("reach snapshot used a different fixture manifest")
    reach_programs = {program["program"]: program for program in reach["programs"]}
    if set(reach_programs) != set(programs):
        raise ValueError("reach snapshot program IDs differ")
    objects = Objects()
    ledger = json.loads(objects.read("13cd313", "docs/compiler-coverage.json"))
    ledger_tests = {f"{row['target']}::{row['name']}": row for row in ledger["tests"]}
    runtime = {}
    report_records = []
    additional_nonruntime = []
    for ref in reviews["runtime_reports"]:
        report_text = objects.read(ref["revision"], ref["path"])
        report = json.loads(report_text)
        baseline = report.get("fixed_revision", report.get("baseline_revision", report.get("legacy_revision")))
        if baseline != manifest["baseline_revision"]:
            raise ValueError("runtime report legacy baseline differs")
        report_records.append({**ref, "sha256": digest(report_text)})
        if ref["format"] == "frontend-review":
            for program in report["programs"]:
                original = programs[program["id"]]
                if program["legacy_tests"] != original["legacy_tests"]:
                    raise ValueError("frontend review test IDs differ")
                for file in program["files"]:
                    expected = next(value for value in original["files"] if value["name"] == file["name"])
                    path = "crates/emela-emb-compiler/tests/compat/legacy-corpus/" + file["fixture"]
                    if expected["sha256"] != file["sha256"] or digest(objects.read(ref["revision"], path)) != file["sha256"]:
                        raise ValueError("frontend review original source hash differs")
            known_hashes = {source.get("sha256") for source in corpus_document["source_inputs"]}
            for case in report["additional_nonruntime_legacy_cases"]:
                old = corpus[case["legacy_test"]]
                for file in case.get("files", []):
                    if file["sha256"] not in known_hashes or digest(objects.read(ref["revision"], file["path"])) != file["sha256"]:
                        raise ValueError("bundled parse source differs from the fixed input")
                additional_nonruntime.append({**case, "legacy_classes": old["legacy_classes"], "report": ref})
            continue
        if ref["format"] == "traits":
            entries = [{"id": report["program"], "files": [{"name": "main.emel", "fixture": report["source_path"], "sha256": report["source_sha256"]}], "legacy_tests": [test for observation in report["observations"] for test in observation["legacy_tests"]], "observations": report["observations"], "runtime_assertions": report["observations"]}]
        elif ref["format"] in ("const-rest", "integration", "entry-programs"):
            entries = [{"id": entry["program"], **entry} for entry in report["entries"]]
        elif ref["format"] == "source-cases":
            entries = []
            for case in report["cases"]:
                runtime_programs = set()
                for variant in case["runtime_variants"]:
                    if variant["status"] != "runtime-passed":
                        raise ValueError(f"runtime case has a pending variant: {case['legacy_test']}")
                    entries.append({"id": variant["program"], "legacy_tests": [case["legacy_test"]], "files": case["original_files"][variant["program"]], "observations": variant, "new_test": variant["new_test"], "runtime_assertions": case["legacy_C_assertions"], "source_adaptation": case["source_adaptation"]})
                    runtime_programs.add(variant["program"])
                for identifier in sorted(set(case["programs"]) - runtime_programs):
                    artifacts = [observation for observation in case.get("artifact_observations", []) if observation.get("program") == identifier]
                    if not artifacts:
                        raise ValueError(f"unexecuted program lacks old artifact evidence: {identifier}")
                    entries.append({"id": identifier, "legacy_tests": [case["legacy_test"]], "files": case["original_files"][identifier], "observations": artifacts, "assessment": "original-body-artifact-evidence", "source_adaptation": case["source_adaptation"]})
        elif ref["format"] == "cleanup-cases":
            entries = []
            for case in report["cases"]:
                if case["status"] != "source-runtime-verified" or set(case["seams"]) - {"private_C_cancel"}:
                    raise ValueError(f"cleanup report contains an unverified or IR-adapted case: {case['legacy_test']}")
                for identifier in sorted({file["program"] for file in case["source_files"]}):
                    entries.append({"id": identifier, "legacy_tests": [case["legacy_test"]], "files": [file for file in case["source_files"] if file["program"] == identifier], "observations": {key: case[key] for key in ("C_generation", "strict_C_objects", "C_runtime", "successful_iterations", "terminal_defect_observed", "old_resource_budgets_preserved")}, "new_test": case["new_test"], "options": case["old_options"], "C_harness": case["harness"], "private_C_seam": case["seams"], "runtime_assertions": case["legacy_C_assertions"], "source_adaptation": case["source_adaptation"]})
        elif ref["format"] == "lifecycle":
            entries = [{"id": entry["program"], "files": [{"name": "main.emel", "fixture": entry["source_path"], "sha256": entry["source_sha256"]}], **entry} for entry in report["legacy_source_runtime"] if "source_sha256" in entry]
        else:
            entries = report["programs"]
        for program in entries:
            original = programs[program["id"]]
            if not set(program["legacy_tests"]).issubset(original["legacy_tests"]):
                raise ValueError(f"runtime legacy test IDs differ: {program['id']}")
            assessment = program.get("assessment", "original-body-runtime-evidence")
            seam = program.get("post_lowering_ir_seam", program.get("post_lowering_ir_seams", program.get("GC_adaptation")))
            if ref["format"] == "integration":
                seam = program["adaptation"]
            if ref["format"] != "child-waiters":
                for file in program["files"]:
                    expected = next(f for f in original["files"] if f["name"] == file["name"])
                    path = file["fixture"]
                    if path.startswith("sources/"):
                        path = "crates/emela-emb-compiler/tests/compat/legacy-corpus/" + path
                    source_revision = report["compiler_checkpoint"] if ref["format"] == "lifecycle" else ref["revision"]
                    if file["sha256"] != expected["sha256"] or digest(objects.read(source_revision, path)) != file["sha256"]:
                        raise ValueError(f"runtime input hash differs: {program['id']}")
                pure = not seam
                if program.get("C_harness_sha256") and digest(objects.read(ref["revision"], program["C_harness"])) != program["C_harness_sha256"]:
                    raise ValueError(f"runtime C harness hash differs: {program['id']}")
                if program.get("route") == "existing-source-observation-replacement":
                    assessment = "existing-source-C-observations-match"
                if program["id"] == "program:5ce8d2ba40fb42a01466":
                    # The old test rejects the runtime-const input under ISR policy,
                    # then executes its separate static-const alternative.
                    assessment = "original-body-diagnostic-evidence"
            else:
                adapted = objects.read(ref["revision"], program["source"])
                if digest(adapted) != program["sha256"]:
                    raise ValueError(f"adapted runtime source hash differs: {program['id']}")
                unexported = re.sub(r"(?m)^@export\([^\n]*\)\n", "", adapted)
                if len(original["files"]) != 1 or digest(unexported) != original["files"][0]["sha256"]:
                    raise ValueError(f"removing only C export does not restore original: {program['id']}")
                pure = program["observations"]["pure_source_runtime"]
            pipeline = "source-to-C-artifact-check" if assessment == "original-body-artifact-evidence" else "source-check-to-ISR-rejection" if assessment == "original-body-diagnostic-evidence" else "existing-source-C-observation-replacement" if assessment == "existing-source-C-observations-match" else "source-to-C" if pure else "source-IR-adaptation-to-C"
            evidence = {"report": ref, "assessment": assessment, "legacy_tests": program["legacy_tests"], "original_body_hash_verified": True, "pipeline": pipeline, "observations": program.get("observations", program.get("scope")), "post_lowering_ir_seam": seam, "runtime_assertions": program.get("runtime_assertions"), "direct_test": program.get("new_test", program.get("evidence")), "source_adaptation": program.get("source_adaptation", program.get("adaptation")), "configuration": {key: program[key] for key in ("options", "C_harness", "private_C_seam", "old_resource_budgets_preserved") if key in program}, "report_validation": report.get("validation", report.get("run", report.get("execution")))}
            for identifier in program["legacy_tests"]:
                runtime.setdefault(program["id"], {})[identifier] = evidence
    cases = []
    for case in manifest["cases"]:
        identifier = case["legacy_test"]
        old = corpus[identifier]
        families = [family for family in reviews["families"] if re.search(family["path_pattern"], case["legacy_path"])]
        if not families:
            raise ValueError(f"unassigned normal source case: {identifier}")
        relevant = [{"program": p, **runtime[p][identifier]} for p in case["programs"] if p in runtime and identifier in runtime[p]]
        all_runtime = len(relevant) == len(case["programs"])
        manual = reviews["reviewed_cases"].get(identifier)
        ledger_row = ledger_tests.get(identifier)
        ledger_final = bool(ledger_row and ledger_row["status"] in ("verified", "replaced"))
        if all_runtime:
            decisions = {entry["assessment"] for entry in relevant}
            assessment = "original-body-artifact-evidence" if decisions == {"original-body-artifact-evidence"} else "existing-source-C-observations-match" if "existing-source-C-observations-match" in decisions else "original-body-runtime-and-diagnostic-evidence" if "original-body-diagnostic-evidence" in decisions else "original-body-runtime-and-artifact-evidence" if "original-body-artifact-evidence" in decisions else "original-body-runtime-evidence"
        else:
            assessment = manual["assessment"] if manual else "existing-ledger-source-replacement" if ledger_final else "runtime-observation-mapping-open"
        cases.append({
            "legacy_test": identifier,
            "legacy_path": old["path"],
            "legacy_line": old["line"],
            "programs": case["programs"],
            "original_files": {p: programs[p]["files"] for p in case["programs"]},
            "legacy_frontend_expectations": case["legacy_expected"],
            "legacy_runtime_call_sites": old["backend_expectations"],
            "legacy_execution_configuration": case["execution_configuration"],
            "assessment": assessment,
            "manual_observation_review": manual,
            "existing_ledger_decision": {k: ledger_row[k] for k in ("status", "new_tests", "evidence", "reason")} if ledger_row else None,
            "families": [family["id"] for family in families],
            "runtime_evidence": relevant,
            "active_assignments": [{"program": p, **reviews["active_program_assignments"][p]} for p in case["programs"] if p in reviews["active_program_assignments"]],
            "reach_snapshot": {p: reach_record(reach_programs[p]) for p in case["programs"]},
            "final_integrated_verification": "required",
        })
    normal_ids = {case["legacy_test"] for case in cases}
    extra_ids = {case["legacy_test"] for case in additional_nonruntime}
    expected_extra = {identifier for identifier, case in corpus.items() if case["category"] == "source"} - normal_ids
    if extra_ids != expected_extra:
        raise ValueError("additional rejection/parse-only source cases differ from the fixed corpus")
    doc = {
        "schema_version": 1,
        "legacy_revision": manifest["baseline_revision"],
        "fixture_manifest_sha256": digest(fixture_text),
        "review_sha256": digest(args.reviews.read_text()),
        "reach_snapshot": {"path": str(args.reach), "sha256": digest(reach_text), "compiler_revision": reach["compiler_revision"], "probe_options": reach["probe_options"], "runtime_verified": False},
        "policy": "Every normal legacy source test is retained. Compile acceptance and C objects are not C runtime evidence. Candidate source/IR tests are partial until old assertions are matched. Shared programs and generated variants are counted separately from distinct tests.",
        "counts": {"normal_legacy_tests": len(cases), "programs": len(programs), "additional_nonruntime_legacy_cases": len(additional_nonruntime), "normal_cases_with_open_observations": sum(case["assessment"] == "runtime-observation-mapping-open" for case in cases), "assessment": dict(sorted(Counter(case["assessment"] for case in cases).items())), "families": {family["id"]: sum(family["id"] in case["families"] for case in cases) for family in reviews["families"]}, "raw_reach": dict(Counter(p["raw"]["status"] for p in reach["programs"])), "concrete_root_reviewed_issues": dict(Counter(category(root) for p in reach["programs"] for root in p["public_root_probes"] if root["status"] == "compiler-rejected"))},
        "families": reviews["families"],
        "runtime_reports": report_records,
        "additional_nonruntime_legacy_cases": additional_nonruntime,
        "cases": cases,
    }
    generated = json.dumps(doc, ensure_ascii=False, indent=2) + "\n"
    if args.check:
        if args.out.read_text() != generated:
            raise ValueError("source runtime crosswalk differs")
    else:
        args.out.write_text(generated)
    print(json.dumps(doc["counts"], ensure_ascii=False))


if __name__ == "__main__":
    main()
