#!/usr/bin/env python3
"""Index fixed legacy test sources without importing or executing legacy code.

Only ``git ls-tree`` and ``git show`` read the reference repository. Rust comments
and strings are tokenized before discovering tests, modules, calls and fixtures.
This is an evidence inventory, not a Rust compiler or a claim that tests ran.
"""
from __future__ import annotations

import argparse
import bisect
import hashlib
import json
import posixpath
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

REVISION = "c34c0aeefdd966d854371b3110db00a47798751d"
CRATE = "crates/emela-codegen-emb"
ROOTS = [f"{CRATE}/tests", f"{CRATE}/src/compiler/tests"]


@dataclass(frozen=True)
class Token:
    kind: str
    text: str
    start: int
    end: int
    value: str | None = None


def decode_string(text: str) -> str:
    result = []
    index = 0
    escapes = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}
    while index < len(text):
        if text[index] != "\\":
            result.append(text[index])
            index += 1
            continue
        index += 1
        char = text[index]
        if char in "\r\n":
            while index < len(text) and text[index].isspace():
                index += 1
        elif char == "x":
            result.append(chr(int(text[index + 1:index + 3], 16)))
            index += 3
        elif char == "u" and text[index + 1] == "{":
            close = text.index("}", index + 2)
            result.append(chr(int(text[index + 2:close].replace("_", ""), 16)))
            index = close + 1
        else:
            if char not in escapes:
                raise ValueError(f"unsupported Rust string escape: \\{char}")
            result.append(escapes[char])
            index += 1
    return "".join(result)


def tokenize(source: str) -> list[Token]:
    tokens = []
    index = 0
    while index < len(source):
        start = index
        if source[index].isspace():
            index += 1
            continue
        if source.startswith("//", index):
            end = source.find("\n", index)
            index = len(source) if end < 0 else end
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while depth:
                if index >= len(source):
                    raise ValueError("unterminated Rust block comment")
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue
        raw = re.match(r'(?:[bc])?r(\#*)"', source[index:])
        if raw:
            delimiter = '"' + raw[1]
            content = index + raw.end()
            close = source.find(delimiter, content)
            if close < 0:
                raise ValueError("unterminated Rust raw string")
            index = close + len(delimiter)
            kind = "string" if source[start] == "r" else "bytes"
            tokens.append(Token(kind, source[start:index], start, index, source[content:close]))
            continue
        prefix = 1 if source[index:index + 2] in ('b"', 'c"') else 0
        if source[index + prefix:index + prefix + 1] == '"':
            content = index + prefix + 1
            index = content
            while index < len(source):
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    break
                else:
                    index += 1
            if index >= len(source):
                raise ValueError("unterminated Rust string")
            value = decode_string(source[content:index])
            index += 1
            tokens.append(Token("bytes" if prefix else "string", source[start:index], start, index, value))
            continue
        if source[index] == "'":
            # A lifetime has no closing quote; a character literal has exactly one
            # scalar or an escape before its closing quote.
            char = re.match(r"'(?:[^'\\\r\n]|\\(?:[nrt0\\'\"]|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f_]+\}))'", source[index:])
            if char:
                index += char.end()
                tokens.append(Token("character", source[start:index], start, index))
                continue
        identifier = re.match(r"(?:r\#)?[A-Za-z_][A-Za-z_0-9]*", source[index:])
        if identifier:
            index += identifier.end()
            tokens.append(Token("identifier", source[start:index], start, index))
            continue
        pair = source[index:index + 2]
        index += 2 if pair in ("::", "->", "=>", "&&", "||", "..", "==", "!=", "<=", ">=") else 1
        tokens.append(Token("punctuation", source[start:index], start, index))
    return tokens


class GitObjects:
    def __init__(self, repository: Path):
        self.repository = repository
        raw = self.git("ls-tree", "-r", "-z", REVISION)
        self.tree = {}
        for entry in raw.split(b"\0"):
            if not entry:
                continue
            metadata, path = entry.split(b"\t", 1)
            _, kind, oid = metadata.decode().split()
            if kind == "blob":
                self.tree[path.decode()] = oid
        self.cache: dict[str, str] = {}

    def git(self, *arguments: str) -> bytes:
        return subprocess.check_output(["git", "-C", str(self.repository), *arguments])

    def read(self, path: str) -> str:
        if path not in self.tree:
            raise FileNotFoundError(path)
        if path not in self.cache:
            self.cache[path] = self.git("show", f"{REVISION}:{path}").decode("utf-8")
        return self.cache[path]


class RustFile:
    def __init__(self, path: str, source: str):
        self.path = path
        self.source = source
        self.tokens = tokenize(source)
        self.lines = [-1] + [match.start() for match in re.finditer("\n", source)]
        self.mates: dict[int, int] = {}
        stack = []
        closes = {")": "(", "]": "[", "}": "{"}
        for index, token in enumerate(self.tokens):
            if token.text in ("(", "[", "{"):
                stack.append(index)
            elif token.text in closes:
                if not stack or self.tokens[stack[-1]].text != closes[token.text]:
                    raise ValueError(f"unbalanced Rust delimiters at {path}:{self.line(token.start)}")
                open_ = stack.pop()
                self.mates[open_] = index
                self.mates[index] = open_
        if stack:
            raise ValueError(f"unclosed Rust delimiters: {path}")

    def line(self, offset: int) -> int:
        return bisect.bisect_left(self.lines, offset)

    def raw(self, start: int, end: int) -> str:
        if start >= end:
            return ""
        return self.source[self.tokens[start].start:self.tokens[end - 1].end]

    def split(self, start: int, end: int) -> list[tuple[int, int]]:
        result = []
        first = start
        index = start
        while index < end:
            if self.tokens[index].text in ("(", "[", "{"):
                index = self.mates[index] + 1
            elif self.tokens[index].text == ",":
                if first < index:
                    result.append((first, index))
                first = index + 1
                index += 1
            else:
                index += 1
        if first < end:
            result.append((first, end))
        return result


@dataclass
class Function:
    id: str
    file: RustFile
    target: str
    module: tuple[str, ...]
    name: str
    start: int
    body_start: int
    end: int
    parameters: list[str]
    attributes: list[str]
    inherited: list[str]

    @property
    def test(self) -> bool:
        return any(re.fullmatch(r"#\s*\[\s*test\s*\]", attr) for attr in self.attributes)

    @property
    def qualified_name(self) -> str:
        return "::".join((*self.module, self.name))


class Inventory:
    def __init__(self, objects: GitObjects):
        self.objects = objects
        self.files: dict[str, RustFile] = {}
        self.functions: dict[str, Function] = {}
        self.by_symbol: dict[tuple[str, tuple[str, ...], str], str] = {}
        self.contexts: dict[str, list[dict]] = {}
        self.edges: list[dict] = []
        self.unresolved: list[dict] = []
        self.active: set[tuple[str, str, tuple[str, ...]]] = set()

    def rust(self, path: str) -> RustFile:
        if path not in self.files:
            self.files[path] = RustFile(path, self.objects.read(path))
        return self.files[path]

    def discover(self) -> None:
        self.walk(f"{CRATE}/src/lib.rs", f"{CRATE}/src", "lib", (), [])
        for path in sorted(self.objects.tree):
            if posixpath.dirname(path) == f"{CRATE}/tests" and path.endswith(".rs"):
                self.walk(path, f"{CRATE}/tests", posixpath.basename(path)[:-3], (), [])

    def walk(self, path: str, directory: str, target: str, prefix: tuple[str, ...], inherited: list[str]) -> None:
        key = (path, target, prefix)
        if key in self.active:
            self.unresolved.append({"kind": "recursive-module", "path": path, "target": target, "module": list(prefix)})
            return
        if path not in self.objects.tree:
            self.unresolved.append({"kind": "missing-module", "path": path, "target": target, "module": list(prefix)})
            return
        self.active.add(key)
        file = self.rust(path)
        context = {"target": target, "module": "::".join(prefix), "attributes": inherited}
        if context not in self.contexts.setdefault(path, []):
            self.contexts[path].append(context)
            self.region(file, directory, target, prefix, inherited, 0, len(file.tokens))
        self.active.remove(key)

    def region(self, file: RustFile, directory: str, target: str, prefix: tuple[str, ...], inherited: list[str], start: int, end: int) -> None:
        tokens = file.tokens
        attributes = []
        index = start
        while index < end:
            token = tokens[index].text
            if token == "#" and index + 1 < end and tokens[index + 1].text == "[":
                close = file.mates[index + 1]
                attributes.append(file.raw(index, close + 1))
                index = close + 1
                continue
            if token == "pub" and index + 1 < end and tokens[index + 1].text == "(":
                index = file.mates[index + 1] + 1
                continue
            if token == "mod" and index + 2 < end and tokens[index + 1].kind == "identifier":
                name = tokens[index + 1].text.removeprefix("r#")
                child = (*prefix, name)
                marker = tokens[index + 2].text
                if marker == "{":
                    close = file.mates[index + 2]
                    self.region(file, posixpath.join(directory, name), target, child, inherited + attributes, index + 3, close)
                    index = close + 1
                elif marker == ";":
                    explicit = next((match[1] for attr in attributes if (match := re.fullmatch(r'#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]', attr))), None)
                    if explicit is not None:
                        path = posixpath.normpath(posixpath.join(posixpath.dirname(file.path), explicit))
                    else:
                        flat = posixpath.join(directory, f"{name}.rs")
                        path = flat if flat in self.objects.tree else posixpath.join(directory, name, "mod.rs")
                    self.edges.append({"kind": "module", "from": file.path, "line": file.line(tokens[index].start), "target": target, "module": "::".join(child), "to": path, "attributes": attributes})
                    child_directory = posixpath.dirname(path) if posixpath.basename(path) == "mod.rs" else path[:-3]
                    self.walk(path, child_directory, target, child, inherited + attributes)
                    index += 3
                else:
                    index += 1
                attributes = []
                continue
            if token == "fn" and index + 1 < end and tokens[index + 1].kind == "identifier":
                name = tokens[index + 1].text.removeprefix("r#")
                parameter_open = index + 2
                angle = 0
                while parameter_open < end:
                    text = tokens[parameter_open].text
                    if text == "<": angle += 1
                    if text == ">": angle -= 1
                    if text == "(" and angle == 0: break
                    parameter_open += 1
                if parameter_open >= end: raise ValueError(f"function parameter list not found: {file.path}:{name}")
                parameter_close = file.mates[parameter_open]
                body = parameter_close + 1
                while body < end and tokens[body].text not in ("{", ";"):
                    if tokens[body].text in ("(", "["): body = file.mates[body]
                    body += 1
                if body >= end: raise ValueError(f"function body not found: {file.path}:{name}")
                if tokens[body].text == "{":
                    close = file.mates[body]
                    parameters = []
                    for first, last in file.split(parameter_open + 1, parameter_close):
                        colon = next((cursor for cursor in range(first, last) if tokens[cursor].text == ":"), last)
                        identifiers = [tokens[cursor].text for cursor in range(first, colon) if tokens[cursor].kind == "identifier" and tokens[cursor].text != "mut"]
                        parameters.append(identifiers[-1] if identifiers else f"$pattern_{len(parameters)}")
                    qualified = "::".join((*prefix, name))
                    identifier = f"{target}::{qualified}@{file.path}:{file.line(tokens[index].start)}"
                    function = Function(identifier, file, target, prefix, name, index, body + 1, close, parameters, attributes, inherited)
                    if identifier in self.functions: raise ValueError(f"duplicate function context: {identifier}")
                    self.functions[identifier] = function
                    self.by_symbol[(target, prefix, name)] = identifier
                    index = close + 1
                else:
                    index = body + 1
                attributes = []
                continue
            if token == "include" and index + 3 < end and tokens[index + 1].text == "!" and tokens[index + 2].text in ("(", "{"):
                close = file.mates[index + 2]
                argument = tokens[index + 3]
                if argument.kind == "string" and index + 4 == close:
                    path = posixpath.normpath(posixpath.join(posixpath.dirname(file.path), argument.value))
                    self.edges.append({"kind": "include-rust", "from": file.path, "line": file.line(tokens[index].start), "target": target, "module": "::".join(prefix), "to": path})
                    self.walk(path, directory, target, prefix, inherited + attributes)
                else:
                    self.unresolved.append({"kind": "generated-rust-include", "path": file.path, "line": file.line(tokens[index].start), "expression": file.raw(index, close + 1)})
                index = close + 1
                attributes = []
                continue
            if token in ("{", "(", "["):
                close = file.mates[index]
                # Do not mistake macro-generated syntax for physical test definitions.
                tests = [cursor for cursor in range(index, close) if tokens[cursor].text == "#" and cursor + 2 < close and tokens[cursor + 1].text == "[" and tokens[cursor + 2].text == "test"]
                if tests:
                    self.unresolved.append({"kind": "test-attributes-in-unparsed-item", "path": file.path, "line": file.line(tokens[index].start), "count": len(tests), "target": target})
                index = close + 1
                attributes = []
                continue
            if token == ";": attributes = []
            index += 1


@dataclass(frozen=True)
class Expression:
    function: str
    start: int
    end: int


@dataclass
class Call:
    name: str
    start: int
    close: int
    arguments: list[tuple[int, int]]
    methods: list[str]


def calls(function: Function) -> Iterable[Call]:
    file = function.file
    tokens = file.tokens
    for index in range(function.body_start, function.end):
        if tokens[index].text != "(" or index == 0:
            continue
        last = index - 1
        macro = tokens[last].text == "!"
        if macro:
            last -= 1
        if tokens[last].kind != "identifier":
            continue
        first = last
        while first >= 2 and tokens[first - 1].text in ("::", ".") and tokens[first - 2].kind == "identifier":
            first -= 2
        name = "".join(token.text.removeprefix("r#") for token in tokens[first:last + 1])
        if macro:
            name += "!"
        close = file.mates[index]
        cursor = close + 1
        methods = []
        while cursor + 2 < function.end and tokens[cursor].text == "." and tokens[cursor + 1].kind == "identifier" and tokens[cursor + 2].text == "(":
            methods.append(tokens[cursor + 1].text)
            cursor = file.mates[cursor + 2] + 1
        yield Call(name, first, close, file.split(index + 1, close), methods)


def source_sink(name: str) -> tuple[str, str] | None:
    stem = name.rsplit("::", 1)[-1]
    if stem in ("try_lower", "try_lower_with"):
        return ("files", "checked-frontend-lower-result")
    if stem == "lower_files":
        return ("files", "checked-frontend-lower-success")
    if stem == "resolve_files":
        return ("files", "parse-resolve-success")
    if name.endswith("MemoryFiles::new"):
        return ("files", "source-registration")
    if name == "emela_syntax::parse":
        return ("text", "syntax-result")
    return None


class Evidence:
    def __init__(self, inventory: Inventory):
        self.inventory = inventory
        self.objects = inventory.objects
        self.inputs: dict[str, dict] = {}
        self.includes = []
        self.unresolved = []
        self.call_cache = {identifier: list(calls(function)) for identifier, function in inventory.functions.items()}
        self.binding_cache = {}
        self.reassigned: set[tuple[str, str]] = set()
        self.function_names: dict[str, list[str]] = {}
        for identifier, function in inventory.functions.items():
            self.function_names.setdefault(function.name, []).append(identifier)

    def reference(self, file: RustFile, first: int, last: int) -> dict:
        return {"path": file.path, "line": file.line(file.tokens[first].start), "end_line": file.line(file.tokens[last - 1].end), "rust_expression": file.raw(first, last)}

    def source(self, text: str, origin: dict, kind: str = "literal") -> str:
        digest = hashlib.sha256(text.encode()).hexdigest()
        identifier = f"source:{digest[:20]}"
        entry = self.inputs.setdefault(identifier, {"id": identifier, "kind": "concrete", "sha256": digest, "utf8_bytes": len(text.encode()), "source": text, "origins": [], "new_status": "unmapped", "new_evidence": []})
        assert entry["sha256"] == digest
        origin = {"kind": kind, **origin}
        if origin not in entry["origins"]:
            entry["origins"].append(origin)
        return identifier

    def generated(self, origin: dict, reason: str) -> str:
        digest = hashlib.sha256(json.dumps(origin, sort_keys=True).encode()).hexdigest()
        identifier = f"generated:{digest[:20]}"
        self.inputs.setdefault(identifier, {"id": identifier, "kind": "generated-or-unresolved", "origin": origin, "review_reason": reason, "new_status": "unmapped", "new_evidence": []})
        return identifier

    def bindings(self, function: Function) -> dict[str, list[Expression]]:
        if function.id in self.binding_cache:
            return self.binding_cache[function.id]
        file = function.file
        tokens = file.tokens
        result: dict[str, list[Expression]] = {}
        for index in range(function.body_start, function.end):
            if tokens[index].text not in ("let", "const", "for"):
                continue
            kind = tokens[index].text
            cursor = index + 1
            if cursor < function.end and tokens[cursor].text == "mut":
                cursor += 1
            if cursor >= function.end:
                continue
            # Single-name bindings and for loops are exact. Destructuring is left
            # explicit for review instead of guessing a projection.
            if tokens[cursor].kind != "identifier":
                continue
            name = tokens[cursor].text
            marker = cursor + 1
            while marker < function.end and tokens[marker].text not in ("=", "in", ";", "{"):
                if tokens[marker].text in ("(", "["):
                    marker = file.mates[marker]
                marker += 1
            if marker >= function.end or tokens[marker].text != ("in" if kind == "for" else "="):
                continue
            first = marker + 1
            last = first
            while last < function.end and tokens[last].text not in (("{",) if kind == "for" else (";",)):
                if tokens[last].text in ("(", "[", "{"):
                    last = file.mates[last]
                last += 1
            if first < last:
                result.setdefault(name, []).append(Expression(function.id, first, last))
        for index in range(function.body_start, function.end - 2):
            if tokens[index].kind != "identifier" or tokens[index + 1].text != "=" or tokens[index].text not in result:
                continue
            if index > function.body_start and tokens[index - 1].text in ("let", "mut", "const", "."):
                continue
            last = index + 2
            while last < function.end and tokens[last].text != ";":
                if tokens[last].text in ("(", "[", "{"):
                    last = file.mates[last]
                last += 1
            result[tokens[index].text].append(Expression(function.id, index + 2, last))
            self.reassigned.add((function.id, tokens[index].text))
        for values in result.values():
            values.sort(key=lambda expression: expression.start)
        self.binding_cache[function.id] = result
        return result

    def helper(self, function: Function, name: str) -> str | None:
        if "!" in name or "." in name:
            return None
        parts = name.split("::")
        for length in range(len(function.module), -1, -1):
            prefix = list(function.module[:length])
            rest = list(parts)
            if rest[0] == "crate":
                prefix = []
                rest.pop(0)
            elif rest[0] == "self":
                rest.pop(0)
            while rest and rest[0] == "super":
                if prefix: prefix.pop()
                rest.pop(0)
            if not rest:
                continue
            key = (function.target, tuple(prefix + rest[:-1]), rest[-1])
            if key in self.inventory.by_symbol:
                return self.inventory.by_symbol[key]
        # Imported helpers can be shared by several targets. A unique physical
        # definition is still unambiguous; target contexts remain in its trace.
        candidates = self.function_names.get(parts[-1], [])
        physical = {(self.inventory.functions[candidate].file.path, self.inventory.functions[candidate].start) for candidate in candidates}
        if len(physical) == 1 and candidates:
            return next((candidate for candidate in candidates if self.inventory.functions[candidate].target == function.target), candidates[0])
        return None

    def strip(self, expression: Expression) -> Expression:
        function = self.inventory.functions[expression.function]
        file = function.file
        first, last = expression.start, expression.end
        while last - first >= 4:
            suffix = file.raw(first, last)
            matched = False
            for pattern in (r"\.iter\(\)\.copied\(\)$", r"\.as_str\(\)$", r"\.as_slice\(\)$", r"\.to_owned\(\)$", r"\.to_string\(\)$"):
                match = re.search(pattern, re.sub(r"\s+", "", suffix))
                if match:
                    # All accepted suffix tokens are punctuation/identifiers, so
                    # removing their non-whitespace token count is unambiguous.
                    last -= len(tokenize(match[0]))
                    matched = True
                    break
            if not matched:
                break
        while first < last:
            if file.tokens[first].text == "&":
                first += 1
                continue
            if file.tokens[first].text == "(" and file.mates[first] == last - 1:
                # Parenthesized file tuples must remain distinguishable.
                if len(file.split(first + 1, last - 1)) == 1:
                    first += 1
                    last -= 1
                    continue
            break
        return Expression(expression.function, first, last)

    def evaluate(self, expression: Expression, role: str, environment: dict[str, Expression], active: frozenset[tuple] = frozenset()) -> list[dict]:
        expression = self.strip(expression)
        function = self.inventory.functions[expression.function]
        file = function.file
        tokens = file.tokens
        first, last = expression.start, expression.end
        key = (function.id, first, last, role)
        if key in active:
            return [{"input": self.generated(self.reference(file, first, last), "recursive Rust binding"), "filename": None}]
        active = active | {key}
        if first >= last:
            return []
        # Apply explicit immutable string transformations; never silently treat
        # an included asset as the final source when a postfix changes it.
        cursor = first
        replacements = []
        while cursor < last:
            if tokens[cursor].text == "." and cursor + 2 < last and tokens[cursor + 1].text == "replace" and tokens[cursor + 2].text == "(":
                if file.mates[cursor + 2] == last - 1:
                    replacements.append(cursor)
            if tokens[cursor].text in ("(", "[", "{"):
                cursor = file.mates[cursor]
            cursor += 1
        if replacements:
            marker = replacements[-1]
            arguments = file.split(marker + 3, last - 1)
            if len(arguments) == 2 and all(end == start + 1 and tokens[start].kind == "string" for start, end in arguments):
                before, after = (tokens[start].value for start, _ in arguments)
                values = self.evaluate(Expression(function.id, first, marker), role, environment, active)
                if all(self.inputs[value["input"]]["kind"] == "concrete" for value in values):
                    return [{**value, "input": self.source(self.inputs[value["input"]]["source"].replace(before, after), {**self.reference(file, first, last), "original_input": value["input"], "replacement": [before, after]}, "constant-string-replace")} for value in values]
        if tokens[first].text == "[" and file.mates[first] + 5 == last:
            close = file.mates[first]
            if [token.text for token in tokens[close + 1:last]] == [".", "concat", "(", ")"]:
                values = self.evaluate(Expression(function.id, first, close + 1), "text", environment, active)
                if all(self.inputs[value["input"]]["kind"] == "concrete" for value in values):
                    text = "".join(self.inputs[value["input"]]["source"] for value in values)
                    origin = {**self.reference(file, first, last), "components": [value["input"] for value in values]}
                    return [{"input": self.source(text, origin, "constant-array-concat"), "filename": None}]
        if first + 1 == last and tokens[first].kind == "identifier":
            name = tokens[first].text
            if name in environment:
                return self.evaluate(environment[name], role, {}, active)
            bindings = self.bindings(function).get(name, [])
            previous = [binding for binding in bindings if binding.start < first]
            if previous:
                if (function.id, name) in self.reassigned:
                    result = []
                    for binding in previous:
                        for value in self.evaluate(binding, role, environment, active):
                            result.append({**value, "binding_review": "reassigned/conditional Rust variable: alternatives are retained without claiming which branch executed"})
                    return result
                return self.evaluate(previous[-1], role, environment, active)
            # A file-level const can hold shared source text.
            for index in range(0, function.start):
                if tokens[index].text == "const" and index + 1 < len(tokens) and tokens[index + 1].text == name:
                    marker = next((cursor for cursor in range(index + 2, function.start) if tokens[cursor].text == "="), None)
                    if marker is not None:
                        end = next(cursor for cursor in range(marker + 1, function.start) if tokens[cursor].text == ";")
                        return self.evaluate(Expression(function.id, marker + 1, end), role, environment, active)
        if tokens[first].text == "[" and file.mates[first] == last - 1:
            result = []
            for start, end in file.split(first + 1, last - 1):
                result.extend(self.evaluate(Expression(function.id, start, end), role, environment, active))
            return result
        if role == "files" and tokens[first].text == "(" and file.mates[first] == last - 1:
            members = file.split(first + 1, last - 1)
            if len(members) == 2:
                name_start, name_end = members[0]
                if name_end == name_start + 1 and tokens[name_start].kind == "string":
                    filename = tokens[name_start].value
                    if not filename.endswith(".emel"):
                        return []
                    values = self.evaluate(Expression(function.id, *members[1]), "text", environment, active)
                    return [{**value, "filename": filename} for value in values]
        if role == "text" and first + 1 == last and tokens[first].kind == "string":
            identifier = self.source(tokens[first].value, self.reference(file, first, last))
            return [{"input": identifier, "filename": None}]
        if role == "text" and first + 3 < last and tokens[first].text == "include_str" and tokens[first + 1].text == "!" and tokens[first + 3].kind == "string" and file.mates[first + 2] == last - 1:
            requested = tokens[first + 3].value
            path = posixpath.normpath(posixpath.join(posixpath.dirname(file.path), requested))
            if path in self.objects.tree:
                identifier = self.source(self.objects.read(path), {"path": path, "git_blob": self.objects.tree[path], "included_from": self.reference(file, first, last)}, "included-asset")
                return [{"input": identifier, "filename": None}]
        origin = self.reference(file, first, last)
        return [{"input": self.generated(origin, f"static {role} expression requires review; it remains an audit obligation"), "filename": None}]

    def expectations(self, sink: tuple[str, str], methods: list[str]) -> dict:
        _, semantic = sink
        reject = any(method in ("unwrap_err", "expect_err", "is_err", "err") for method in methods)
        accept = any(method in ("unwrap", "expect", "is_ok", "unwrap_or_else") for method in methods)
        if semantic == "checked-frontend-lower-success":
            return {"parse": "accept", "resolve": "accept", "typecheck": "accept", "lower": "accept", "basis": "lower_files unwraps the checked lowering result"}
        if semantic == "checked-frontend-lower-result":
            return {"parse": "accept", "resolve": "accept", "typecheck": "accept", "lower": "reject" if reject else "accept" if accept else "review", "basis": "try_lower asserts syntax/resolution/type success; postfix methods describe only its lowering Result"}
        if semantic == "parse-resolve-success":
            return {"parse": "accept", "resolve": "accept", "typecheck": "not-established", "basis": "resolve_files asserts syntax and resolution success"}
        return {"frontend": "review", "basis": "registration or parsing alone does not establish successful compilation"}

    def test(self, test: Function) -> dict:
        invocations = []
        visited = set()
        helper_trace = []
        ir_evidence = []
        backend_expectations = []
        unresolved_calls = []
        candidates = set()
        non_program_literals = []

        def visit(function: Function, environment: dict[str, Expression], trace: list[str]) -> None:
            key = (function.id, tuple(sorted((name, expression.function, expression.start, expression.end) for name, expression in environment.items())))
            if key in visited:
                return
            visited.add(key)
            if len(trace) >= 32:
                unresolved_calls.append({"reason": "recursive/helper trace limit", "trace": trace})
                return
            if function.id not in helper_trace:
                helper_trace.append(function.id)
                for index in range(function.body_start, function.end):
                    token = function.file.tokens[index]
                    if token.kind == "string" and re.search(r"(?m)^\s*(?:pub\s+)?(?:suspend\s+)?(?:fn\s+[a-z_]|(?:effect|handler|error|type|enum|layer)\s+[A-Z]|import\s+[A-Z]|const\s+[A-Z])", token.value):
                        transformed = any(call.name.rsplit(".", 1)[-1] == "replace" and any(start <= index < end for start, end in call.arguments) for call in self.call_cache[function.id])
                        identifier = self.source(token.value, self.reference(function.file, index, index + 1), "string-replacement-operand" if transformed else "source-shaped-literal-candidate")
                        if transformed:
                            non_program_literals.append({"input": identifier, "role": "string replacement operand, not a complete source program"})
                        else:
                            candidates.add(identifier)
            for call in self.call_cache[function.id]:
                sink = source_sink(call.name)
                if sink and call.arguments:
                    values = self.evaluate(Expression(function.id, *call.arguments[0]), sink[0], environment)
                    if values:
                        invocations.append({"callee": call.name, "path": function.file.path, "line": function.file.line(function.file.tokens[call.start].start), "trace": trace + [function.id], "inputs": values, "postfix_methods": call.methods, "legacy_expected": self.expectations(sink, call.methods)})
                    continue
                if any(marker in call.name for marker in ("FnBuilder::new", "Module::new", "compile_flow", "verify_flow", "flow_from_core")):
                    evidence = {"callee": call.name, "path": function.file.path, "line": function.file.line(function.file.tokens[call.start].start)}
                    if evidence not in ir_evidence:
                        ir_evidence.append(evidence)
                if call.name.rsplit("::", 1)[-1] in ("compile", "compile_flow", "compile_and_run", "run_native"):
                    backend_expectations.append({"callee": call.name, "path": function.file.path, "line": function.file.line(function.file.tokens[call.start].start), "postfix_methods": call.methods, "basis": "call-site expectation only; no legacy or new execution was performed by this indexer"})
                helper = self.helper(function, call.name)
                if helper:
                    target = self.inventory.functions[helper]
                    if not (target.file.path.startswith(f"{CRATE}/tests/") or target.file.path.startswith(f"{CRATE}/src/compiler/tests/") or target.file.path.startswith(f"{CRATE}/examples/support/")):
                        # Public compiler APIs are boundaries, not helper bodies.
                        # Walking their implementation would attach every backend
                        # function and unrelated branch to every source test.
                        continue
                    arguments = {parameter: Expression(function.id, *argument) for parameter, argument in zip(target.parameters, call.arguments)}
                    # Resolve caller parameters before changing the lexical environment.
                    for parameter, expression in list(arguments.items()):
                        caller_tokens = function.file.tokens
                        if expression.end == expression.start + 1 and caller_tokens[expression.start].text in environment:
                            arguments[parameter] = environment[caller_tokens[expression.start].text]
                    visit(target, arguments, trace + [function.id])

        visit(test, {}, [])
        concrete = sorted({value["input"] for invocation in invocations for value in invocation["inputs"] if self.inputs[value["input"]]["kind"] == "concrete"})
        generated = sorted({value["input"] for invocation in invocations for value in invocation["inputs"] if self.inputs[value["input"]]["kind"] != "concrete"})
        unassigned = sorted(candidates.difference(concrete))
        category = "source" if invocations else "IR-only" if ir_evidence and not unassigned else "needs-review"
        review = bool(generated or unassigned or unresolved_calls or category == "needs-review" or any("review" in invocation["legacy_expected"].values() or any("binding_review" in value for value in invocation["inputs"]) for invocation in invocations))
        legacy_classes = []
        if any(invocation["legacy_expected"].get("lower") == "accept" for invocation in invocations):
            legacy_classes.append("source-success-expectation")
        if any(invocation["legacy_expected"].get("lower") == "reject" for invocation in invocations):
            legacy_classes.append("source-lowering-rejection-expectation")
        if any(invocation["legacy_expected"].get("parse") == "accept" and "typecheck" not in invocation["legacy_expected"] for invocation in invocations):
            legacy_classes.append("source-resolution-only-expectation")
        if not legacy_classes:
            legacy_classes.append("IR-only" if category == "IR-only" else "needs-review")
        return {"id": f"{test.target}::{test.qualified_name}", "target": test.target, "name": test.qualified_name, "path": test.file.path, "line": test.file.line(test.file.tokens[test.start].start), "end_line": test.file.line(test.file.tokens[test.end].end), "attributes": test.attributes, "module_attributes": test.inherited, "ignored": any("ignore" in attribute for attribute in test.attributes + test.inherited), "category": category, "legacy_classes": legacy_classes, "review_status": "needs-review" if review else "static-evidence-recorded", "source_inputs": concrete, "generated_inputs": generated, "unassigned_source_literal_candidates": unassigned, "non_program_literals": non_program_literals, "source_invocations": invocations, "backend_expectations": backend_expectations, "IR_evidence": ir_evidence, "helper_trace": helper_trace, "unresolved_calls": unresolved_calls, "new_status": "unmapped", "new_evidence": []}

    def build(self) -> dict:
        tests = [self.test(test) for test in sorted(self.inventory.functions.values(), key=lambda function: (function.target, function.qualified_name, function.file.path, function.start)) if test.test]
        for path, file in self.inventory.files.items():
            tokens = file.tokens
            for index in range(len(tokens) - 3):
                if tokens[index].text not in ("include_str", "include_bytes") or tokens[index + 1].text != "!" or tokens[index + 2].text not in ("(", "{"):
                    continue
                close = file.mates[index + 2]
                argument = tokens[index + 3]
                edge = {"from": path, "line": file.line(tokens[index].start), "macro": tokens[index].text, "expression": file.raw(index, close + 1)}
                if index + 4 == close and argument.kind == "string":
                    target = posixpath.normpath(posixpath.join(posixpath.dirname(path), argument.value))
                    edge.update({"requested": argument.value, "to": target, "exists_in_fixed_tree": target in self.objects.tree, "git_blob": self.objects.tree.get(target)})
                    if target.endswith(".emel") and target in self.objects.tree:
                        edge["source_input"] = self.source(self.objects.read(target), {"path": target, "git_blob": self.objects.tree[target], "included_from": {"path": path, "line": edge["line"]}}, "included-asset")
                else:
                    edge["review_reason"] = "nonliteral asset path"
                self.includes.append(edge)
        primary = sorted(path for path in self.objects.tree if path.endswith(".rs") and any(path.startswith(root + "/") for root in ROOTS))
        unvisited = [path for path in primary if path not in self.inventory.files]
        files = [{"path": path, "git_blob": self.objects.tree[path], "sha256": hashlib.sha256(file.source.encode()).hexdigest(), "utf8_bytes": len(file.source.encode()), "requested_root": any(path.startswith(root + "/") for root in ROOTS), "contexts": self.inventory.contexts.get(path, []), "physical_test_attributes": sum(tokens.text == "#" and index + 2 < len(file.tokens) and file.tokens[index + 1].text == "[" and file.tokens[index + 2].text == "test" for index, tokens in enumerate(file.tokens))} for path, file in sorted(self.inventory.files.items())]
        counts = {"requested_rust_files": len(primary), "discovered_rust_files": len(files), "test_contexts": len(tests), "physical_test_functions": len({(test["path"], test["line"]) for test in tests})}
        context_ids = sorted({identifier for test in tests if test["generated_inputs"] for invocation in test["source_invocations"] if any(value["input"] in test["generated_inputs"] for value in invocation["inputs"]) for identifier in invocation["trace"]})
        contexts = []
        for identifier in context_ids:
            function = self.inventory.functions[identifier]
            contexts.append({"id": identifier, "path": function.file.path, "line": function.file.line(function.file.tokens[function.start].start), "parameters": function.parameters, "rust_function": function.file.raw(function.start, function.end + 1)})
        return {"schema_version": 1, "baseline_revision": REVISION, "purpose": "Fixed-object test/source evidence inventory. Legacy expectations are read from code, not results of executing the old compiler. No record verifies the new compiler.", "requested_roots": ROOTS, "discovery": {"method": "Rust string/comment tokenization, Cargo default lib/integration roots, mod/path/include traversal, helper call traces and literal binding evaluation", "counts": counts, "unvisited_requested_rust_files": unvisited, "unresolved_module_items": self.inventory.unresolved, "limitations": ["Conditional attributes are retained; disabled configurations are not claimed to have executed.", "Generated/unsupported Rust expressions remain explicit review obligations; no source is excluded because the new compiler lacks syntax.", "IR-only means no source sink was found on the resolved helper trace, not that source coverage follows from an IR test.", "Registration, parsing, typechecking, lowering and C execution expectations are separate."]}, "files": files, "module_edges": self.inventory.edges, "asset_includes": sorted(self.includes, key=lambda edge: (edge["from"], edge["line"])), "tests": tests, "source_inputs": sorted(self.inputs.values(), key=lambda source: source["id"]), "generated_input_review_contexts": contexts}


def apply_reviews(document: dict, evidence: Evidence, path: Path) -> None:
    reviews = json.loads(path.read_text(encoding="utf-8"))
    if reviews["baseline_revision"] != REVISION:
        raise ValueError("review revision differs from the fixed corpus revision")
    tests = {test["id"]: test for test in document["tests"]}
    for identifier, review in reviews["tests"].items():
        if identifier not in tests:
            raise ValueError(f"review refers to an undiscovered test: {identifier}")
        test = tests[identifier]
        if review["category"] != "source" and test["source_invocations"]:
            raise ValueError(f"review would discard a source route: {identifier}")
        test["route_review"] = review
        test["category"] = review["category"]
        test["legacy_classes"] = review.get("legacy_classes", [review["category"]])
        if review.get("non_program_literal_language"):
            test["non_program_literals"].extend({"input": value, "role": f"reviewed {review['non_program_literal_language']} fixture, not Emela source"} for value in test["unassigned_source_literal_candidates"])
            test["unassigned_source_literal_candidates"] = []
        if "source_assets" in review:
            values = []
            for asset in review["source_assets"]:
                value = evidence.source(evidence.objects.read(asset["path"]), {"path": asset["path"], "git_blob": evidence.objects.tree[asset["path"]], "review": identifier}, "reviewed-bundle-member")
                values.append({"input": value, "filename": asset["module"]})
            for generated in test["generated_inputs"]:
                evidence.inputs[generated]["resolved_by_review"] = identifier
            test["generated_inputs"] = []
            test["source_inputs"] = sorted(value["input"] for value in values)
            for invocation in test["source_invocations"]:
                invocation["inputs"] = values
        if "legacy_expected" in review:
            for invocation in test["source_invocations"]:
                invocation["legacy_expected"] = {**review["legacy_expected"], "basis": review["note"]}
        test["review_status"] = "route-reviewed-input-pending" if test["generated_inputs"] or test["unassigned_source_literal_candidates"] else "legacy-route-reviewed"
    document["source_inputs"] = sorted(evidence.inputs.values(), key=lambda source: source["id"])
    document["route_reviews"] = {"path": path.as_posix(), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "test_count": len(reviews["tests"])}


def finish(document: dict) -> None:
    tests = document["tests"]
    inputs = {value["id"]: value for value in document["source_inputs"]}
    if len({test["id"] for test in tests}) != len(tests):
        raise ValueError("duplicate canonical test IDs")
    attribute_count = sum(file["physical_test_attributes"] for file in document["files"])
    if attribute_count != document["discovery"]["counts"]["physical_test_functions"]:
        raise ValueError("not every tokenized #[test] attribute has a discovered physical function")
    if document["discovery"]["unvisited_requested_rust_files"] or document["discovery"]["unresolved_module_items"]:
        raise ValueError("test/module discovery has unresolved coverage")
    program_inputs = set()
    generated_inputs = set()
    for test in tests:
        for identifier in test["source_inputs"]:
            if identifier not in inputs or inputs[identifier]["kind"] != "concrete":
                raise ValueError(f"invalid source reference: {identifier}")
            program_inputs.add(identifier)
        generated_inputs.update(test["generated_inputs"])
    for entry in inputs.values():
        if entry["kind"] == "concrete" and hashlib.sha256(entry["source"].encode()).hexdigest() != entry["sha256"]:
            raise ValueError(f"source content hash mismatch: {entry['id']}")
        entry["is_resolved_program_input"] = entry["id"] in program_inputs
    counts = document["discovery"]["counts"]
    counts.update({
        "tokenized_test_attributes": attribute_count,
        "source_test_contexts": sum(test["category"] == "source" for test in tests),
        "IR_only_test_contexts": sum(test["category"] == "IR-only" for test in tests),
        "runtime_only_test_contexts": sum(test["category"] == "runtime-only" for test in tests),
        "infrastructure_test_contexts": sum(test["category"] == "infrastructure-contract" for test in tests),
        "source_success_expectation_tests": sum("source-success-expectation" in test["legacy_classes"] for test in tests),
        "source_rejection_expectation_tests": sum("source-rejection-expectation" in test["legacy_classes"] or "source-lowering-rejection-expectation" in test["legacy_classes"] for test in tests),
        "review_test_contexts": sum(test["review_status"] in ("needs-review", "route-reviewed-input-pending") for test in tests),
        "resolved_program_inputs": len(program_inputs),
        "concrete_text_records_including_fragments_and_candidates": sum(value["kind"] == "concrete" for value in inputs.values()),
        "generated_or_unresolved_program_inputs": len(generated_inputs),
        "ignored_test_contexts": sum(test["ignored"] for test in tests),
    })


def self_test() -> None:
    source = '''// #[test] fn fake() {}\n/* /* nested */ #[test] */
const TEXT: &str = r###"#[test] fn fake() { \" }"###;
#[test] fn real<'a>(text: &'a str) { let ch = '\\n'; let text = "a\\n\\
  b"; }
'''
    file = RustFile("probe.rs", source)
    assert sum(token.text == "test" for token in file.tokens) == 1
    assert [token.value for token in file.tokens if token.kind == "string"][-1] == "a\nb"
    assert decode_string(r"\u{1f600}\x41\0") == "😀A\0"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("legacy_repository", type=Path)
    parser.add_argument("--output", type=Path, default=Path("docs/compiler-legacy-source-corpus.json"))
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--reviews", type=Path, default=Path("docs/tools/legacy_corpus_reviews.json"))
    arguments = parser.parse_args()
    self_test()
    objects = GitObjects(arguments.legacy_repository)
    inventory = Inventory(objects)
    inventory.discover()
    evidence = Evidence(inventory)
    document = evidence.build()
    apply_reviews(document, evidence, arguments.reviews)
    finish(document)
    content = json.dumps(document, ensure_ascii=False, indent=2) + "\n"
    if arguments.check:
        if arguments.output.read_text(encoding="utf-8") != content:
            raise SystemExit(f"stale corpus index: {arguments.output}")
    else:
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(content, encoding="utf-8", newline="\n")
    print(json.dumps(document["discovery"], ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
