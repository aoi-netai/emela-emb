#!/usr/bin/env python3
"""Survey saved legacy sources with the independent compiler and concrete C roots.

This tool never reads the old repository. Compiler rejection is recorded as an
unmet obligation, not a passing rejection test. C object checks do not claim C
execution. Raw source and each explicit C-root adaptation are reported apart.
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


def source_mask(source: str) -> str:
    """Hide comments/strings, including nested interpolation and multiline strings."""
    def interpolation_end(index: int, depth: int) -> int:
        braces = 0
        while index < len(source):
            if source[index] == '"':
                index = string_end(index, depth + 1)
                continue
            if source[index] == "#":
                newline = source.find("\n", index)
                index = len(source) if newline < 0 else newline
                continue
            if source[index] == "{":
                braces += 1
            if source[index] == "}":
                if not braces:
                    return index + 1
                braces -= 1
            index += 1
        raise ValueError("unterminated interpolation in accepted source")

    def string_end(index: int, depth: int) -> int:
        if depth >= 256:
            raise ValueError("string nesting limit exceeded")
        delimiter = '"""' if source.startswith('"""', index) else '"'
        index += len(delimiter)
        while index < len(source):
            if source.startswith(delimiter, index):
                return index + len(delimiter)
            if source.startswith("#{", index):
                index = interpolation_end(index + 2, depth + 1)
            elif source[index] == "\\":
                index += 2
            else:
                index += 1
        raise ValueError("unterminated string in accepted source")

    mask = list(source)
    index = 0
    while index < len(source):
        if source[index] == "#":
            close = source.find("\n", index)
            close = len(source) if close < 0 else close
        elif source[index] == '"':
            close = string_end(index, 0)
        else:
            index += 1
            continue
        for cursor in range(index, min(close, len(source))):
            if mask[cursor] != "\n":
                mask[cursor] = " "
        index = close
    return "".join(mask)


def public_candidates(source: str) -> list[dict]:
    mask = source_mask(source)
    depth = []
    value = 0
    for character in mask:
        depth.append(value)
        if character == "{": value += 1
        if character == "}": value -= 1
    result = []
    pattern = r"\b(?:(pub)\s+)?(?:(suspend)\s+)?fn\s+([A-Za-z_][A-Za-z_0-9]*)"
    for match in re.finditer(pattern, mask):
        public, _, name = match.groups()
        if depth[match.start()] != 0 or not (public or name == "main"):
            continue
        tail = mask[match.end():].lstrip()
        line_start = source.rfind("\n", 0, match.start()) + 1
        previous = source[:line_start].rstrip().rsplit("\n", 1)[-1]
        reason = "explicit-generic-header-needs-concrete-ABI-adapter" if tail.startswith("[") else "external-declaration-needs-ABI-review" if previous.lstrip().startswith("@external") else None
        result.append({"name": name, "position": match.start(), "source_line": source.count("\n", 0, match.start()) + 1, "add_pub": not bool(public), "nonconcrete_reason": reason})
    return result


def classify(diagnostics: str) -> str:
    if any(name in diagnostics for name in ("Embedded.", "Embedded/")):
        return "P7-embedded-services-root-owned"
    if any(name in diagnostics for name in ("Std.Fiber", "Std.Deferred", "Std.Cause", "Std.Scope", "type 'Fiber", "type 'Deferred")):
        return "P6-source-not-connected-at-tested-revision"
    if "suspend" in diagnostics or "effect 'Async'" in diagnostics:
        return "P5-source-not-connected-at-tested-revision"
    if "generic" in diagnostics and ("export" in diagnostics or "C boundary" in diagnostics):
        return "concrete-ABI-adapter-required"
    if "adapter" in diagnostics:
        return "probe-Options-adapter-required"
    return "P7-review-required"


def compile_case(binary: Path, files: list[tuple[str, str]], entry: str, directory: Path, heap: int, capacity: int, root: dict | None) -> dict:
    directory.mkdir()
    names = []
    for name, source in files:
        path = directory / "inputs" / name
        if Path(name).is_absolute() or ".." in Path(name).parts:
            raise ValueError(f"invalid logical fixture path: {name}")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(source, encoding="utf-8", newline="")
        names.append(path)
    output = directory / "generated"
    command = [str(binary), "build", *map(str, names), "--entry", str(directory / "inputs" / entry), "--out-dir", str(output), "--heap-bytes", str(heap), "--string-capacity", str(capacity)]
    if root is not None:
        command += ["--entry-function", "legacy_probe_no_implicit_main"]
    try:
        process = subprocess.run(command, capture_output=True, text=True, timeout=60)
    except subprocess.TimeoutExpired:
        return {"status": "compiler-timeout", "runtime_verified": False}
    diagnostics = process.stderr.replace(str(directory / "inputs") + "/", "").replace(str(directory), "$probe")
    if process.returncode:
        error_lines = [line for line in diagnostics.splitlines() if "error[" in line]
        backend_only = bool(error_lines) and all("error[backend]" in line for line in error_lines)
        return {"status": "compiler-rejected", "frontend": "accepted-before-backend-rejection" if backend_only else "not-established-by-this-probe", "C_generation": "rejected" if backend_only else "not-reached", "issue_category": classify(diagnostics), "diagnostics": diagnostics, "runtime_verified": False}
    artifact = json.loads((output / "emela_artifact.json").read_text(encoding="utf-8"))
    result = {"status": "source-accepted", "frontend": "accepted", "C_generation": "accepted", "entry": artifact["metadata"]["entry"], "exports": artifact["metadata"]["exports"], "runtime_verified": False}
    if not result["entry"] and not result["exports"]:
        result["C_objects"] = {"status": "no-selected-concrete-root"}
        return result
    modes = []
    for optimization in ("-O0", "-O2"):
        translation_units = []
        for index, path in enumerate(sorted(output.rglob("*.c"))):
            command = ["cc", "-std=c11", "-Wall", "-Werror", optimization, "-I", str(output), "-c", str(path), "-o", str(directory / f"object-{optimization[1:]}-{index}.o")]
            process = subprocess.run(command, capture_output=True, text=True, timeout=60)
            translation_units.append({"path": path.relative_to(output).as_posix(), "success": process.returncode == 0, "diagnostics": process.stderr.replace(str(directory), "$probe")})
        if not translation_units:
            raise ValueError("compiler produced no C translation units")
        modes.append({"optimization": optimization, "translation_units": translation_units})
    result["C_objects"] = {"status": "compiled" if all(unit["success"] for mode in modes for unit in mode["translation_units"]) else "rejected", "modes": modes}
    return result


def survey(program: dict, fixture: Path, binary: Path, heap: int, capacity: int) -> dict:
    files = []
    for entry in program["files"]:
        data = (fixture / entry["fixture"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != entry["sha256"]:
            raise ValueError(f"fixture bytes differ: {entry['fixture']}")
        files.append((entry["name"], data.decode("utf-8")))
    record = {"program": program["id"], "legacy_tests": program["legacy_tests"], "raw_source_preserved": True, "public_root_probes": []}
    with tempfile.TemporaryDirectory(prefix="emela-legacy-reach-") as temporary:
        directory = Path(temporary)
        record["raw"] = compile_case(binary, files, program["entry"], directory / "raw", heap, capacity, None)
        if record["raw"]["status"] != "source-accepted":
            return record
        entry_source = next(source for name, source in files if name == program["entry"])
        for index, root in enumerate(public_candidates(entry_source)):
            root = {key: value for key, value in root.items()}
            if root["nonconcrete_reason"]:
                record["public_root_probes"].append({"root": root, "status": "concrete-ABI-adapter-required", "runtime_verified": False})
                continue
            symbol = f"legacy_{program['id'].split(':')[1]}_{root['name']}"
            position = root["position"]
            annotation = f'@export(c,"{symbol}")\n' + ("pub " if root["add_pub"] else "")
            adapted = entry_source[:position] + annotation + entry_source[position:]
            probe_files = [(name, adapted if name == program["entry"] else source) for name, source in files]
            result = compile_case(binary, probe_files, program["entry"], directory / f"root-{index}", heap, capacity, root)
            root.update({"symbol": symbol, "adaptation": "explicit export for one concrete entry-module root; private main made public only for this ABI probe"})
            record["public_root_probes"].append({"root": root, **result})
    return record


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, default=Path("target/release/emela-emb"))
    parser.add_argument("--compiler-revision", required=True)
    parser.add_argument("--fixture", type=Path, default=Path("crates/emela-emb-compiler/tests/compat/legacy-corpus"))
    parser.add_argument("--out", type=Path, default=Path("docs/compiler-legacy-source-reach.json"))
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--heap-bytes", type=int, default=131072)
    parser.add_argument("--string-capacity", type=int, default=256)
    arguments = parser.parse_args()
    manifest = json.loads((arguments.fixture / "manifest.json").read_text(encoding="utf-8"))
    document = {"schema_version": 1, "baseline_revision": manifest["baseline_revision"], "compiler_revision": arguments.compiler_revision, "compiler_binary_sha256": hashlib.sha256(arguments.compiler.read_bytes()).hexdigest(), "fixture_manifest_sha256": hashlib.sha256((arguments.fixture / "manifest.json").read_bytes()).hexdigest(), "probe_options": {"heap_bytes": arguments.heap_bytes, "string_capacity": arguments.string_capacity, "entry": "main for raw source; disabled for explicit root probes", "scope": "reach survey; legacy-specific Options and Flow postprocessing remain separate in fixture manifest", "public_root_selector": "entry-module main or pub concrete function; explicit generic and external declarations require separate review; backend concrete-export acceptance checks inferred genericity"}, "runtime_verified": False, "programs": []}
    with ThreadPoolExecutor(max_workers=arguments.workers) as workers:
        tasks = [workers.submit(survey, program, arguments.fixture, arguments.compiler.resolve(), arguments.heap_bytes, arguments.string_capacity) for program in manifest["programs"]]
        for index, task in enumerate(as_completed(tasks), 1):
            document["programs"].append(task.result())
            document["programs"].sort(key=lambda value: value["program"])
            document["completed_programs"] = index
            document["total_programs"] = len(tasks)
            arguments.out.parent.mkdir(parents=True, exist_ok=True)
            arguments.out.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
            if index % 10 == 0 or index == len(tasks):
                print(f"{index}/{len(tasks)} programs surveyed", flush=True)


if __name__ == "__main__":
    main()
