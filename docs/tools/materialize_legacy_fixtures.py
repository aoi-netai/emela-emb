#!/usr/bin/env python3
"""Save normal legacy source cases from the reviewed index, without reading old Git."""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path


def build(index: dict) -> tuple[dict, dict[str, bytes]]:
    texts = {entry["id"]: entry for entry in index["source_inputs"]}
    programs = {}
    cases = []
    outputs = {}
    for test in index["tests"]:
        if "source-success-expectation" not in test["legacy_classes"]:
            continue
        if test["generated_inputs"] or test["unassigned_source_literal_candidates"]:
            raise ValueError(f"normal source has unresolved input: {test['id']}")
        references = set()
        for invocation in test["source_invocations"]:
            alternatives = {}
            names = []
            for value in invocation["inputs"]:
                filename = value.get("filename") or "main.emel"
                if filename not in alternatives:
                    alternatives[filename] = []
                    names.append(filename)
                if value["input"] not in alternatives[filename]:
                    alternatives[filename].append(value["input"])
            if not names:
                continue
            for combination in itertools.product(*(alternatives[name] for name in names)):
                files = []
                for name, identifier in zip(names, combination):
                    entry = texts[identifier]
                    if entry["kind"] != "concrete":
                        raise ValueError(f"nonconcrete fixture: {identifier}")
                    filename = f"sources/{entry['sha256']}.emel"
                    contents = entry["source"].encode("utf-8")
                    if hashlib.sha256(contents).hexdigest() != entry["sha256"]:
                        raise ValueError(f"source hash mismatch: {identifier}")
                    outputs[filename] = contents
                    files.append({"name": name, "fixture": filename, "source_id": identifier, "sha256": entry["sha256"]})
                identity = {"entry": names[0], "files": [{"name": value["name"], "sha256": value["sha256"]} for value in files]}
                digest = hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
                program = f"program:{digest[:20]}"
                record = programs.setdefault(program, {"id": program, "entry": names[0], "files": files, "legacy_tests": [], "new_status": "unmapped", "new_evidence": []})
                if test["id"] not in record["legacy_tests"]:
                    record["legacy_tests"].append(test["id"])
                references.add(program)
        if not references:
            raise ValueError(f"normal source test has no program: {test['id']}")
        cases.append({"legacy_test": test["id"], "programs": sorted(references), "legacy_path": test["path"], "legacy_line": test["line"], "legacy_expected": [invocation["legacy_expected"] for invocation in test["source_invocations"]], "execution_configuration": test.get("execution_configuration"), "new_status": "unmapped", "new_evidence": []})
    manifest = {
        "schema_version": 1,
        "baseline_revision": index["baseline_revision"],
        "purpose": "Independent, byte-preserved inputs for every reviewed normal-source legacy test. Compilation, C emission and runtime expectations are separate obligations.",
        "counts": {"normal_legacy_tests": len(cases), "distinct_programs": len(programs), "source_files_by_content": len(outputs)},
        "programs": sorted(programs.values(), key=lambda value: value["id"]),
        "cases": cases,
    }
    if len(cases) != 174:
        raise ValueError(f"fixed normal-source test set changed: {len(cases)} != 174")
    return manifest, outputs


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--index", type=Path, default=Path("docs/compiler-legacy-source-corpus.json"))
    parser.add_argument("--out", type=Path, default=Path("crates/emela-emb-compiler/tests/compat/legacy-corpus"))
    parser.add_argument("--check", action="store_true")
    arguments = parser.parse_args()
    manifest, outputs = build(json.loads(arguments.index.read_text(encoding="utf-8")))
    manifest["index_sha256"] = hashlib.sha256(arguments.index.read_bytes()).hexdigest()
    outputs["manifest.json"] = (json.dumps(manifest, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    for filename, contents in outputs.items():
        path = arguments.out / filename
        if arguments.check:
            if path.read_bytes() != contents:
                raise ValueError(f"stale corpus fixture: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
    actual = {path.relative_to(arguments.out).as_posix() for path in arguments.out.rglob("*") if path.is_file() and path.suffix in (".emel", ".json")}
    if actual != set(outputs):
        raise ValueError(f"unaccounted fixture files: {sorted(actual.symmetric_difference(outputs))}")
    print(json.dumps(manifest["counts"], ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
