#!/usr/bin/env python3
"""Derive a per-method / per-key accept-table from the legacy C++ control server.

The legacy server (`oracle-old/linux-port/gui/ControlSocket.cpp`) validates
almost no request parameter. Nearly every parameter read goes through one of
four defaulting accessors on `JsonObj`, each shaped `if (!has(k)) return d;`,
plus a small number of hand-rolled raw-JSON reads. A misspelled key, a
wrong-typed value, or a string the accessor does not recognise does not produce
an error -- it produces a silently substituted default.

This tool parses that file and emits the table a consumer needs in order to pin
a gate against our source rather than against a hand-written list:

    {method: {key: {accessor, default, guarded_by, accepted_shapes}}}

`guarded_by` is the entire safety story: an unguarded memory-path `addr` and a
`has()`-guarded `enabled` differ ONLY in whether a guard sits above the read.
`accepted_shapes` is what makes the table worth building: a gate that checks
key SPELLING alone passes `{"enabled": "on"}`, which is spelled correctly, is
guarded, and still reads FALSE.

The tool is DESCRIPTIVE. `oracle-old` is reference-only; nothing here proposes
or applies a fix to it.

Two independent enumerations run on every invocation:
  * axis A enumerates by ACCESSOR NAME and builds the table;
  * axis B enumerates by THE OBJECT -- every member access and string subscript
    on every identifier that carries request parameters, whatever the member is
    called.
Axis B is not a restatement of axis A: it is the check that catches a read
through a member axis A has never heard of. Any access axis B sees that no
table entry claims is reported as a gap, loudly.

Usage:
    python3 tools/legacy_accept_table.py [--source DIR] [--out FILE]
                                         [--format json|summary]
                                         [--fail-on-gap]

Exit status:
    0  table emitted, parse complete
    1  table emitted, but coverage is incomplete (with --fail-on-gap)
    2  could not read the source at all
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

SOURCE_RELPATH = Path("linux-port/gui/ControlSocket.cpp")
PARAMS_TYPE = "JsonObj"
ACCESSOR_NAMES = ("get", "getInt", "getU32", "getBool")


# ---------------------------------------------------------------------------
# Source acquisition
# ---------------------------------------------------------------------------


def find_oracle_old(explicit: str | None = None) -> Path:
    """Locate the reference `oracle-old` checkout.

    Order: explicit --source, then $ORACLE_OLD, then an upward walk from this
    file looking for a sibling `oracle-old` that actually contains the target.
    We never fall back to a guess: a wrong source silently produces a wrong
    table, so an unresolvable source is an error, not a default.
    """
    # An explicitly named source is authoritative: if it does not hold the
    # target, that is an error. Searching on past it would quietly derive the
    # table from a DIFFERENT tree than the caller named -- the same class of
    # silent substitution this whole table exists to document.
    if explicit:
        c = Path(explicit)
        if (c / SOURCE_RELPATH).is_file():
            return c.resolve()
        raise FileNotFoundError(
            f"--source {explicit} does not contain {SOURCE_RELPATH}")

    candidates: list[Path] = []
    env = os.environ.get("ORACLE_OLD")
    if env:
        candidates.append(Path(env))

    here = Path(__file__).resolve()
    for parent in here.parents:
        candidates.append(parent / "oracle-old")
        candidates.append(parent)

    for c in candidates:
        if (c / SOURCE_RELPATH).is_file():
            return c.resolve()

    raise FileNotFoundError(
        "could not locate oracle-old: no candidate directory contained "
        f"{SOURCE_RELPATH}. Pass --source DIR or set $ORACLE_OLD."
    )


def source_revision(root: Path) -> dict:
    """Record the exact revision the table was derived from.

    A recipe that does not name the revision it was true at degrades into a
    wrong instruction rather than a historical note, so an unavailable revision
    is reported as unavailable -- never as an empty string, and never omitted.
    """
    out: dict = {"repo": str(root), "file": str(SOURCE_RELPATH)}
    try:
        out["revision"] = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    except Exception as exc:  # noqa: BLE001 - reported, not swallowed
        out["revision"] = None
        out["revision_unavailable_reason"] = f"{type(exc).__name__}: {exc}"
        return out
    try:
        dirty = subprocess.run(
            ["git", "-C", str(root), "status", "--porcelain", "--", str(SOURCE_RELPATH)],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        out["source_file_dirty"] = bool(dirty)
    except Exception as exc:  # noqa: BLE001
        out["source_file_dirty"] = None
        out["dirty_check_unavailable_reason"] = f"{type(exc).__name__}: {exc}"
    return out


# ---------------------------------------------------------------------------
# Lexing helpers
# ---------------------------------------------------------------------------


def blank_comments(text: str) -> str:
    """Replace comment bodies with spaces, preserving length and line breaks.

    Byte offsets and line numbers must survive so every finding can cite a real
    line in the real file. String and character literals are respected so a
    `//` inside a literal is not mistaken for a comment.
    """
    out = list(text)
    i, n = 0, len(text)
    state = "code"
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if state == "code":
            if c == "/" and nxt == "/":
                state = "line_comment"
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if c == "/" and nxt == "*":
                state = "block_comment"
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if c == '"':
                state = "string"
            elif c == "'":
                state = "char"
            i += 1
            continue
        if state == "line_comment":
            if c == "\n":
                state = "code"
            else:
                out[i] = " "
            i += 1
            continue
        if state == "block_comment":
            if c == "*" and nxt == "/":
                out[i] = out[i + 1] = " "
                i += 2
                state = "code"
                continue
            if c != "\n":
                out[i] = " "
            i += 1
            continue
        if c == "\\":
            i += 2
            continue
        if (state == "string" and c == '"') or (state == "char" and c == "'"):
            state = "code"
        i += 1
    return "".join(out)


def line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def match_braces(text: str, open_idx: int) -> int:
    """Return the index just past the `}` matching the `{` at open_idx."""
    depth = 0
    i, n = open_idx, len(text)
    while i < n:
        c = text[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        elif c == '"':
            i += 1
            while i < n and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        i += 1
    return -1


def unescape_cpp(s: str) -> str:
    return s.encode("utf-8").decode("unicode_escape")


# ---------------------------------------------------------------------------
# Stage 1 -- accessor semantics, derived from the JsonObj struct
# ---------------------------------------------------------------------------

# Only the predicate-name -> JSON-shape-name MAPPING is knowledge held here.
# WHICH predicates each accessor uses is read out of the source.
PREDICATE_SHAPES = {
    "is_string": "string",
    "is_number": "number",
    "is_number_integer": "number(integer)",
    "is_number_unsigned": "number(unsigned)",
    "is_number_float": "number(float)",
    "is_boolean": "boolean",
    "is_array": "array",
    "is_object": "object",
    "is_null": "null",
}


def parse_cpp_literal(tok: str):
    tok = tok.strip()
    if tok in ("true", "false"):
        return tok == "true"
    if tok.startswith('"') and tok.endswith('"'):
        return unescape_cpp(tok[1:-1])
    m = re.fullmatch(r"0[xX]([0-9a-fA-F]+)", tok)
    if m:
        return int(m.group(1), 16)
    m = re.fullmatch(r"\(?\s*(?:long long|uint32_t|unsigned int|unsigned|int)?\s*\)?\s*"
                     r"(-?\d+)(?:[uUlL]*)", tok)
    if m:
        return int(m.group(1))
    return {"unparsed_cpp_literal": tok}


def find_struct_body(clean: str, name: str) -> tuple[str, int]:
    m = re.search(r"\bstruct\s+" + re.escape(name) + r"\b\s*\{", clean)
    if not m:
        raise ValueError(f"struct {name} not found in source")
    open_idx = m.end() - 1
    end = match_braces(clean, open_idx)
    if end < 0:
        raise ValueError(f"unbalanced braces in struct {name}")
    return clean[open_idx:end], open_idx


def parse_accessors(clean: str) -> dict:
    """Derive each accessor's implicit default and accepted shapes FROM SOURCE.

    Nothing about the accessors is hardcoded: the default comes from the
    signature's default argument, the accepted shapes from the `v.is_X()`
    predicates in the body, the string coercion set from the `s == "..."`
    comparisons, and whether an unaccepted string falls back to the caller's
    default from whether that branch contains a `return d`.
    """
    body, base = find_struct_body(clean, PARAMS_TYPE)
    accessors: dict = {}

    sig_re = re.compile(
        r"^\s*(?P<ret>[A-Za-z_][\w:<>\s*&]*?)\s"
        r"(?P<name>\w+)\s*\((?P<args>[^)]*)\)\s*const\s*\{",
        re.M,
    )
    for m in sig_re.finditer(body):
        name = m.group("name")
        open_idx = m.end() - 1
        end = match_braces(body, open_idx)
        fnbody = body[open_idx:end]

        default = None
        has_default_arg = False
        args = [a.strip() for a in m.group("args").split(",")]
        if len(args) >= 2 and "=" in args[1]:
            has_default_arg = True
            default = parse_cpp_literal(args[1].split("=", 1)[1])

        delegate = None
        dm = re.search(r"return\s*\([^)]*\)\s*(\w+)\s*\(\s*k\b", fnbody)
        if dm and dm.group(1) != name:
            delegate = dm.group(1)

        predicates: list[str] = []
        for pm in re.finditer(r"\bv\.(is_\w+)\s*\(\s*\)", fnbody):
            if pm.group(1) not in predicates:
                predicates.append(pm.group(1))

        string_set = re.findall(r"\bs\s*==\s*\"([^\"]*)\"", fnbody)
        sbranch = None
        sm = re.search(r"if\s*\(\s*v\.is_string\s*\(\s*\)\s*\)", fnbody)
        if sm:
            after = fnbody[sm.end():]
            bm = re.match(r"\s*\{", after)
            if bm:
                bstart = sm.end() + bm.end() - 1
                sbranch = fnbody[bstart: match_braces(fnbody, bstart)]
            else:
                sbranch = after.split(";", 1)[0]

        accessors[name] = {
            "return_type": " ".join(m.group("ret").split()),
            "implicit_default": default,
            "has_default_argument": has_default_arg,
            "delegates_to": delegate,
            "type_predicates": predicates,
            "accepted_json_types": [PREDICATE_SHAPES.get(p, p) for p in predicates],
            "string_coercion_set": string_set or None,
            "string_branch_falls_back_to_default": (
                None if sbranch is None else ("return d" in sbranch)
            ),
            "missing_key_returns_default": bool(
                re.search(r"if\s*\(\s*!\s*has\s*\(\s*k\s*\)\s*\)\s*return\s+d\s*;", fnbody)
            ),
            "source_line": line_of(clean, base + m.start()),
        }

    # `has()` documents the guard's own semantics, which is why it is parsed
    # alongside the value accessors rather than assumed.
    if "has" in accessors:
        hm = re.search(r"\bhas\s*\([^)]*\)\s*const\s*\{", body)
        hbody = body[hm.end() - 1: match_braces(body, hm.end() - 1)]
        accessors["has"]["checks_type"] = bool(
            re.search(r"at\s*\(\s*k\s*\)\s*\.\s*is_(?!null)", hbody))
        accessors["has"]["rejects_null"] = bool(
            re.search(r"!\s*p->at\s*\(\s*k\s*\)\s*\.\s*is_null", hbody))

    for spec in accessors.values():
        d = spec["delegates_to"]
        if d and d in accessors:
            src = accessors[d]
            spec["accepted_json_types"] = list(src["accepted_json_types"])
            spec["type_predicates"] = list(src["type_predicates"])
            spec["string_coercion_set"] = src["string_coercion_set"]
            spec["string_branch_falls_back_to_default"] = \
                src["string_branch_falls_back_to_default"]

    if "getInt" in accessors:
        gi = re.search(r"\bgetInt\s*\([^)]*\)\s*const\s*\{", body)
        gib = body[gi.end() - 1: match_braces(body, gi.end() - 1)]
        prefixes = []
        if re.search(r"s\[0\]\s*==\s*'0'.*?s\[1\]\s*==\s*'x'", gib, re.S):
            prefixes.append("0x")
        if re.search(r"s\[1\]\s*==\s*'X'", gib):
            prefixes.append("0X")
        if re.search(r"s\[0\]\s*==\s*'\$'", gib):
            prefixes.append("$")
        radix = sorted({int(r) for r in re.findall(r"stoll\([^)]*?,\s*(\d+)\)", gib)})
        extra = {
            "string_numeric_prefixes": prefixes or None,
            "string_radices": radix or None,
            "empty_string_returns_default": bool(
                re.search(r"s\.empty\(\)\s*\)?\s*return\s+d\s*;", gib)),
            "parse_failure_returns_default": bool(
                re.search(r"catch\s*\(\s*\.\.\.\s*\)\s*\{\s*return\s+d\s*;", gib)),
        }
        accessors["getInt"].update(extra)
        for other, spec in accessors.items():
            if other != "getInt" and spec.get("delegates_to") == "getInt":
                spec.update(extra)

    return accessors


def accepted_shapes_for(accessor: str, accessors: dict) -> dict:
    """The per-key `accepted_shapes` record, assembled from parsed accessor facts."""
    a = accessors[accessor]
    shapes: dict = {
        "json_types": list(a["accepted_json_types"]),
        "other_json_types_yield": "default",
    }
    if a.get("string_coercion_set"):
        shapes["string_values_accepted"] = list(a["string_coercion_set"])
        falls_back = a["string_branch_falls_back_to_default"]
        shapes["other_string_values_yield"] = "default" if falls_back else False
        # The sharp edge: when the string branch does NOT return `d`, an
        # unrecognised string produces a hard literal that OVERRIDES an explicit
        # default. That is how `{"enabled": "on"}` -- correctly spelled, and
        # past an explicit has() guard -- reads false.
        shapes["other_string_values_ignore_caller_default"] = not falls_back
    if a.get("string_numeric_prefixes"):
        shapes["string_numeric_prefixes"] = list(a["string_numeric_prefixes"])
        shapes["string_radices"] = list(a.get("string_radices") or [])
        shapes["empty_string_yields"] = (
            "default" if a.get("empty_string_returns_default") else "unknown")
        shapes["unparsable_string_yields"] = (
            "default" if a.get("parse_failure_returns_default") else "unknown")
    return shapes


# ---------------------------------------------------------------------------
# Stage 2 -- function inventory and the method surface
# ---------------------------------------------------------------------------


class Function:
    __slots__ = ("name", "body", "body_offset", "start_line", "params_ident")

    def __init__(self, name, body, body_offset, start_line, params_ident):
        self.name = name
        self.body = body
        self.body_offset = body_offset   # absolute offset of `{` in the file
        self.start_line = start_line
        self.params_ident = params_ident


def parse_functions(clean: str) -> dict[str, Function]:
    """Every top-level `static` function that takes a `const JsonObj&`.

    The identifier bound to the params object is read from the signature rather
    than assumed, so a handler naming it something other than `req` is still
    analysed instead of silently reading as parameterless.
    """
    fns: dict[str, Function] = {}
    sig_re = re.compile(
        r"^static\s+[\w:<>,\s*&\[\]]+?\s(?P<name>\w+)\s*\((?P<args>[^)]*)\)\s*\{", re.M)
    for m in sig_re.finditer(clean):
        if PARAMS_TYPE not in m.group("args"):
            continue
        am = re.search(r"const\s+" + PARAMS_TYPE + r"\s*&\s*(\w+)?", m.group("args"))
        ident = am.group(1) if am and am.group(1) else None
        open_idx = m.end() - 1
        end = match_braces(clean, open_idx)
        if end < 0:
            continue
        fns[m.group("name")] = Function(
            m.group("name"), clean[open_idx:end], open_idx,
            line_of(clean, m.start()), ident)
    return fns


def parse_handlers(clean: str) -> dict[str, str]:
    m = re.search(r"\bHandlers\s*\(\s*\)\s*\{", clean)
    if not m:
        raise ValueError("Handlers() not found in source")
    body = clean[m.end() - 1: match_braces(clean, m.end() - 1)]
    return {op: fn for op, fn in re.findall(r"\{\s*\"(\w+)\"\s*,\s*(\w+)\s*\}", body)}


def parse_canonical_map(clean: str) -> tuple[dict[str, str], str]:
    """legacy op -> canonical op, and the namespace prefix -- both from source."""
    mapping: dict[str, str] = {}
    m = re.search(r"\bCanonicalOp\s*\([^)]*\)\s*\{", clean)
    if m:
        body = clean[m.end() - 1: match_braces(clean, m.end() - 1)]
        for legacy, canon in re.findall(r"==\s*\"(\w+)\"\s*\)\s*return\s+\"(\w+)\"", body):
            mapping[legacy] = canon
    prefix = ""
    pm = re.search(r"push_back\(\s*\"([^\"]*)\"\s*\+\s*CanonicalOp", clean)
    if pm:
        prefix = pm.group(1)
    return mapping, prefix


def parse_envelope(clean: str) -> dict:
    """The request envelope: where `params` comes from, and what a bad one becomes.

    A params member that is absent or not an object is replaced wholesale with
    an empty object, so EVERY key on EVERY method then reads its default. That
    is a whole-request defaulting behaviour a per-key table would otherwise miss.
    """
    m = re.search(
        r"json\s+(\w+)\s*=\s*msg\.contains\(\s*\"(\w+)\"\s*\)\s*&&\s*"
        r"msg\[\s*\"\2\"\s*\]\s*\.\s*(is_\w+)\s*\(\s*\)\s*\?\s*"
        r"msg\[\s*\"\2\"\s*\]\s*:\s*json::(\w+)\(\)", clean)
    if not m:
        return {"parsed": False,
                "reason": "could not locate the params extraction in the envelope"}
    return {
        "parsed": True,
        "identifier": m.group(1),
        "envelope_member": m.group(2),
        "required_shape": PREDICATE_SHAPES.get(m.group(3), m.group(3)),
        "substituted_when_absent_or_wrong_shape": (
            {} if m.group(4) == "object" else f"json::{m.group(4)}()"),
        "consequence": ("a params member that is absent or of the wrong shape is "
                        "replaced with an empty object; every key on every method "
                        "then reads its default, with no error"),
        "line": line_of(clean, m.start()),
    }


def parse_predispatch_methods(clean: str) -> dict[str, dict]:
    """Methods answered BEFORE the Handlers() dispatch.

    These are not in Handlers(), so a table built from Handlers() alone omits
    them entirely. Discovered structurally: `if (method == "X") { ... }` blocks
    inside the message handler.
    """
    hm = re.search(r"\bHandleMessage\s*\([^)]*\)\s*\{", clean)
    if not hm:
        return {}
    hstart = hm.end() - 1
    hend = match_braces(clean, hstart)
    body = clean[hstart:hend]

    out: dict[str, dict] = {}
    for m in re.finditer(r"if\s*\(\s*method\s*==\s*\"([^\"]+)\"\s*\)\s*\{", body):
        name = m.group(1)
        bstart = m.end() - 1
        bend = match_braces(body, bstart)
        out[name] = {
            "body": body[bstart:bend],
            "body_offset": hstart + bstart,
            "line": line_of(clean, hstart + m.start()),
        }
    return out


# ---------------------------------------------------------------------------
# Stage 3 -- parameter reads
# ---------------------------------------------------------------------------


def scan_accessor_reads(fn: Function, accessors: dict, clean: str) -> tuple[list, list]:
    """(value_reads, guards) for one function body, via the JsonObj accessors."""
    ident = fn.params_ident
    if not ident:
        return [], []
    esc = re.escape(ident)

    reads = []
    for m in re.finditer(
        esc + r"\.(" + "|".join(ACCESSOR_NAMES) + r")\s*\(\s*\"([^\"]*)\"\s*(,([^;)]*))?\)",
        fn.body,
    ):
        accessor, key, dflt = m.group(1), unescape_cpp(m.group(2)), m.group(4)
        explicit = dflt is not None and dflt.strip() != ""
        reads.append({
            "accessor": accessor,
            "key": key,
            "default_explicit": explicit,
            "default_value": (parse_cpp_literal(dflt) if explicit
                              else accessors[accessor]["implicit_default"]),
            "in_function": fn.name,
            "offset": m.start(),   # within fn.body, for guard-dominance testing
            "line": line_of(clean, fn.body_offset + m.start()),
        })

    guards = []
    for m in re.finditer(esc + r"\.has\s*\(\s*\"([^\"]*)\"\s*\)", fn.body):
        ls = fn.body.rfind("\n", 0, m.start()) + 1
        le = fn.body.find("\n", m.end())
        dom, kind = guard_dominance(fn.body, m.start(), m.end(), esc)
        guards.append({
            "key": unescape_cpp(m.group(1)),
            "expr": fn.body[ls: le if le >= 0 else len(fn.body)].strip(),
            "negated": bool(re.search(r"!\s*$", fn.body[:m.start()])),
            "dominance": dom,          # (start, end) within fn.body, or None
            "dominance_kind": kind,
            "in_function": fn.name,
            "line": line_of(clean, fn.body_offset + m.start()),
        })
    return reads, guards


def match_parens(text: str, open_idx: int) -> int:
    """Index just past the `)` matching the `(` at open_idx."""
    depth = 0
    for i in range(open_idx, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return i + 1
    return -1


def guard_dominance(body: str, has_start: int, has_end: int, esc: str):
    """The region of a function a `has()` test actually protects.

    This is derived from the ENCLOSING BLOCK STRUCTURE, never from whether the
    read carries an explicit default. The two are orthogonal: a read can have an
    explicit default and no guard (`getU32("addr", 0)` in read_vram), or no
    explicit default and a guard (`getInt("value")` inside `if (has("value"))`).
    Inferring either from the other produces false positives AND excludes real
    unguarded sites by construction, so the properties are computed separately.

    Returns ((start, end) within `body`, kind) or (None, reason):
      "early_bail"    `if (!has(k)) return ...;`  -> protects everything after
      "block"         `if (has(k)) { ... }`       -> protects that block only
    """
    # Walk left to the `(` that opens the condition containing this has().
    depth = 0
    open_paren = None
    for i in range(has_start - 1, -1, -1):
        c = body[i]
        if c == ")":
            depth += 1
        elif c == "(":
            if depth == 0:
                open_paren = i
                break
            depth -= 1
    if open_paren is None:
        return None, "no enclosing condition"
    if not re.search(r"\bif\s*$", body[:open_paren]):
        return None, "enclosing call is not an if-condition"

    cond_end = match_parens(body, open_paren)
    if cond_end < 0:
        return None, "unbalanced condition"

    # The `!` sits BEFORE the match, so the text to inspect ends at has_start.
    # Testing body[:has_end] can never match -- it ends with the has() call
    # itself -- which silently classified every early-bail guard as a block
    # guard covering only its own `return`, i.e. as guarding nothing.
    negated = bool(re.search(r"!\s*$", body[:has_start]))

    j = cond_end
    while j < len(body) and body[j].isspace():
        j += 1
    if j < len(body) and body[j] == "{":
        b_start, b_end = j, match_braces(body, j)
    else:
        semi = body.find(";", j)
        b_start, b_end = j, (semi + 1 if semi >= 0 else len(body))
    if b_end < 0:
        return None, "unbalanced guard body"

    if negated:
        # A negated test only guards the rest of the function if it leaves.
        if re.search(r"\breturn\b|\bthrow\b", body[b_start:b_end]):
            return (b_end, len(body)), "early_bail"
        return None, "negated has() that does not return; protects nothing"
    return (b_start, b_end), "block"


def scan_raw_reads(body: str, expr_pat: str, body_offset: int, clean: str,
                   where: str) -> list:
    """Reads that bypass the accessor family, keyed off a raw json expression.

    Recognises `<expr>contains("K")`, `<expr>["K"]`, `<expr>at("K")` and
    nlohmann's own defaulting reader `<expr>value("K", D)`. Every consumed token
    contributes an evidence line, so the object-axis cross-check can tell a read
    this function claimed from one it missed.
    """
    found: dict[str, dict] = {}
    patterns = [
        ("contains", expr_pat + r"contains\s*\(\s*\"([^\"]*)\"\s*\)", None, False),
        ("subscript", expr_pat + r"\[\s*\"([^\"]*)\"\s*\]", None, False),
        ("at", expr_pat + r"at\s*\(\s*\"([^\"]*)\"\s*\)", None, False),
        ("value", expr_pat + r"value\s*\(\s*\"([^\"]*)\"\s*,\s*([^)]*)\)", 2, False),
        # nlohmann's own defaulting reader applied to a NESTED object, e.g.
        # `params["clientCapabilities"].value("events", false)`. Without this the
        # nested key would be silently omitted while its line still counted as
        # claimed -- the exact silent-omission failure this table must not have.
        ("nested_value",
         expr_pat + r"\[\s*\"([^\"]*)\"\s*\]\s*\.\s*value\s*\(\s*\"([^\"]*)\"\s*,\s*([^)]*)\)",
         3, True),
    ]
    for kind, pat, dgroup, nested in patterns:
        for m in re.finditer(pat, body):
            key = (unescape_cpp(m.group(1)) + "." + unescape_cpp(m.group(2))
                   if nested else unescape_cpp(m.group(1)))
            line = line_of(clean, body_offset + m.start())
            rec = found.setdefault(key, {
                "key": key, "kinds": [], "evidence_lines": [],
                "default_value": None, "default_explicit": False, "where": where,
            })
            if kind not in rec["kinds"]:
                rec["kinds"].append(kind)
            if line not in rec["evidence_lines"]:
                rec["evidence_lines"].append(line)
            if dgroup:
                rec["default_value"] = parse_cpp_literal(m.group(dgroup))
                rec["default_explicit"] = True
    for rec in found.values():
        rec["evidence_lines"].sort()
        rec["line"] = rec["evidence_lines"][0]
    return sorted(found.values(), key=lambda r: r["line"])


def scan_type_predicates(body: str, key: str) -> list[str]:
    """Type predicates applied near a key's raw read, e.g. `["k"].is_object()`."""
    preds: list[str] = []
    for m in re.finditer(r"\[\s*\"" + re.escape(key) + r"\"\s*\]\s*\.\s*(is_\w+)\s*\(", body):
        p = PREDICATE_SHAPES.get(m.group(1), m.group(1))
        if p not in preds:
            preds.append(p)
    return preds


def called_param_helpers(fn: Function, helpers: set[str]) -> list[str]:
    """Helper functions this function hands the params object to."""
    if not fn.params_ident:
        return []
    return sorted(
        h for h in helpers
        if h != fn.name
        and re.search(r"\b" + re.escape(h) + r"\s*\(\s*" + re.escape(fn.params_ident) + r"\b",
                      fn.body)
    )


# ---------------------------------------------------------------------------
# Stage 4 -- build the table
# ---------------------------------------------------------------------------


def _record(key, accessor, default, guarded_by, shapes, sites, extra=None):
    rec = {
        "accessor": accessor,
        "default": default,
        "guarded_by": guarded_by,
        "accepted_shapes": shapes,
        "read_sites": sites,
    }
    if extra:
        rec.update(extra)
    return rec


def build_table(source_text: str) -> dict:
    clean = blank_comments(source_text)
    accessors = parse_accessors(clean)
    fns = parse_functions(clean)
    handlers = parse_handlers(clean)
    canon_map, prefix = parse_canonical_map(clean)
    envelope = parse_envelope(clean)
    predispatch = parse_predispatch_methods(clean)

    handler_fns = set(handlers.values())
    helper_fns = {n for n in fns if n not in handler_fns}

    per_fn: dict[str, tuple] = {}
    for name, fn in fns.items():
        reads, guards = scan_accessor_reads(fn, accessors, clean)
        raw_expr = r"(?:\(\s*\*\s*)?" + re.escape(fn.params_ident or "\0") + r"\s*\.\s*p\s*\)?\s*(?:->|\.)?\s*" \
            if fn.params_ident else None
        touches_raw = bool(raw_expr and
                           re.search(re.escape(fn.params_ident) + r"\.p\b", fn.body))
        raws = (scan_raw_reads(fn.body, raw_expr, fn.body_offset, clean, fn.name)
                if touches_raw else [])
        # A raw params touch whose key is not a string literal cannot be
        # attributed. Dropping it would make an unanalysable read read as
        # "this method has no such parameter", so it is recorded as unparsed.
        if touches_raw and not raws:
            hit = re.search(re.escape(fn.params_ident) + r"\.p\b", fn.body)
            raws = [{"key": None, "unparsed": True,
                     "reason": "raw params access with no extractable string key",
                     "kinds": [], "default_value": None, "default_explicit": False,
                     "where": fn.name,
                     "line": line_of(clean, fn.body_offset + hit.start()),
                     "evidence_lines": sorted({
                         line_of(clean, fn.body_offset + m.start()) for m in
                         re.finditer(re.escape(fn.params_ident) + r"\.p\b", fn.body)})}]
        for r in raws:
            if r.get("unparsed"):
                continue
            r["element_predicates"] = [
                PREDICATE_SHAPES.get(p, p)
                for p in re.findall(r"\b\w+\.(is_\w+)\s*\(\s*\)", fn.body)
            ]
            r["container_predicates"] = scan_type_predicates(fn.body, r["key"])
        per_fn[name] = (reads, guards, raws)

    def closure(name: str, seen: set[str] | None = None) -> list[str]:
        seen = seen if seen is not None else set()
        if name in seen or name not in fns:
            return []
        seen.add(name)
        out = [name]
        for h in called_param_helpers(fns[name], helper_fns):
            out.extend(closure(h, seen))
        return out

    # Named canonically, matching the table's own keys: a consumer cross-
    # referencing this list against `methods` must find the same string.
    unresolved_handlers = [
        {"method": prefix + canon_map.get(op, op), "legacy_op": op, "handler": fn,
         "reason": "handler function body not found in source"}
        for op, fn in sorted(handlers.items()) if fn not in fns
    ]

    table: dict = {}
    method_meta: dict = {}
    claimed_lines: set[int] = set()
    unparsed_entries: list = []

    for legacy_op in sorted(handlers):
        fname = handlers[legacy_op]
        canonical_bare = canon_map.get(legacy_op, legacy_op)
        canonical = prefix + canonical_bare
        aliases = sorted({legacy_op, canonical_bare, prefix + legacy_op, canonical})
        method_meta[canonical] = {
            "handler": fname, "legacy_op": legacy_op,
            "accepted_spellings": aliases, "dispatch": "Handlers()",
        }

        if fname not in fns:
            table[canonical] = {
                "__unparsed__": _record(
                    "__unparsed__", None, None, None, None, [],
                    {"unparsed": True,
                     "reason": f"handler {fname} not found in source; parameters "
                               f"UNKNOWN. Treat this method as unanalysed, not as "
                               f"taking no parameters."})
            }
            unparsed_entries.append({"method": canonical, "handler": fname,
                                     "reason": "handler body not found"})
            continue

        chain = closure(fname)
        reads, guards, raws = [], [], []
        for f in chain:
            r, g, w = per_fn[f]
            reads.extend(r)
            guards.extend(g)
            raws.extend(w)

        for g in guards:
            claimed_lines.add(g["line"])

        checks_type = bool(accessors.get("has", {}).get("checks_type"))

        def guard_for(read: dict) -> dict | None:
            """The guard protecting THIS read, by block dominance.

            Same-function guards must lexically dominate the read; a `has()`
            sitting in an unrelated branch protects nothing. A guard in another
            function of the call chain is recorded as transitive, since the read
            is only reachable through the guarded path.
            """
            same, other = [], []
            for g in guards:
                if g["key"] != read["key"]:
                    continue
                (same if g["in_function"] == read["in_function"] else other).append(g)
            for g in same:
                dom = g["dominance"]
                if dom and dom[0] <= read["offset"] < dom[1]:
                    return {
                        "expr": g["expr"], "line": g["line"],
                        "in_function": g["in_function"],
                        "kind": g["dominance_kind"],
                        "negated_early_bail": g["negated"],
                        # The durable point: has() is satisfied by ANY present,
                        # non-null value. It guards against a MISSING key, never
                        # against a malformed one.
                        "guards_against": "absence+type" if checks_type else "absence",
                        "guard_checks_type": checks_type,
                    }
            if other:
                g = other[0]
                return {
                    "expr": g["expr"], "line": g["line"], "in_function": g["in_function"],
                    "kind": "transitive",
                    "negated_early_bail": g["negated"],
                    "guards_against": "absence+type" if checks_type else "absence",
                    "guard_checks_type": checks_type,
                    "note": "the read is in a different function of the call chain, "
                            "reachable only through the guarded path",
                }
            return None

        by_key: dict[str, list] = {}
        for r in reads:
            r["guard"] = guard_for(r)
            by_key.setdefault(r["key"], []).append(r)
            claimed_lines.add(r["line"])

        entry: dict = {}
        for key, sites in sorted(by_key.items()):
            names = sorted({s["accessor"] for s in sites})
            defaults = {json.dumps(s["default_value"], sort_keys=True) for s in sites}
            shapes: dict = {}
            for n in names:
                for sk, sv in accepted_shapes_for(n, accessors).items():
                    if sk == "json_types":
                        acc = shapes.setdefault("json_types", [])
                        acc.extend(t for t in sv if t not in acc)
                    else:
                        shapes[sk] = sv
            extra = ({"multiple_readings": True}
                     if len(names) > 1 or len(defaults) > 1 else None)

            # A key counts as guarded only if EVERY site is. One unguarded path
            # is the path a caller has to plan for, and summarising a mixed key
            # as guarded would hide exactly that path.
            guarded = [s for s in sites if s["guard"]]
            key_guard = guarded[0]["guard"] if len(guarded) == len(sites) else None
            if guarded and len(guarded) != len(sites):
                extra = dict(extra or {})
                extra["partially_guarded"] = True
                extra["unguarded_sites"] = [s["line"] for s in sites if not s["guard"]]

            # Make the "unaccepted value does not yield the declared default"
            # hazard explicit at the key level, where a consumer reads it.
            d0 = sites[0]["default_value"]
            if shapes.get("other_string_values_ignore_caller_default"):
                shapes = dict(shapes)
                shapes["effective_value_for_unlisted_string"] = \
                    shapes.get("other_string_values_yield")
                shapes["declared_default_is_not_applied_to_unlisted_strings"] = True
                shapes["unlisted_string_inverts_declared_default"] = (d0 is True)

            entry[key] = _record(
                key,
                names[0] if len(names) == 1 else names,
                {"value": d0,
                 "explicit": sites[0]["default_explicit"],
                 "source": "call_site" if sites[0]["default_explicit"]
                           else "accessor_signature"},
                key_guard,
                shapes,
                [{"accessor": s["accessor"], "in_function": s["in_function"],
                  "line": s["line"], "default_explicit": s["default_explicit"],
                  "default_value": s["default_value"], "guarded_by": s["guard"]}
                 for s in sorted(sites, key=lambda s: s["line"])],
                extra,
            )

        for w in raws:
            claimed_lines.update(w["evidence_lines"])
            if w.get("unparsed"):
                key = f"__unparsed_raw_read_line_{w['line']}__"
                entry[key] = _record(
                    key, "raw_json", None, None, None,
                    [{"accessor": "raw_json", "in_function": w["where"],
                      "line": ln} for ln in w["evidence_lines"]],
                    {"unparsed": True, "reason": w["reason"]})
                unparsed_entries.append({"method": canonical, "key": None,
                                         "line": w["line"], "reason": w["reason"]})
                continue
            entry[w["key"]] = _record(
                w["key"], "raw_json",
                {"value": w["default_value"] if w["default_explicit"] else [],
                 "explicit": w["default_explicit"],
                 "source": "hand_rolled_read: a failed shape check yields an empty "
                           "result and the call still SUCCEEDS"},
                {"expr": " && ".join(w["kinds"]) + f'("{w["key"]}")',
                 "line": w["line"], "in_function": w["where"],
                 "negated_early_bail": False,
                 "guard_checks_type": True,
                 "note": "type-checked but NOT error-reporting: a wrong shape is "
                         "silently dropped and the method still returns success"},
                {"json_types": w.get("container_predicates") or ["array"],
                 "element_json_types": ["string"] if "string" in
                                       (w.get("element_predicates") or []) else [],
                 "other_json_types_yield": [],
                 "non_matching_elements": "silently dropped"},
                [{"accessor": "raw_json", "in_function": w["where"], "line": ln}
                 for ln in w["evidence_lines"]],
            )

        table[canonical] = entry

    # ---- methods answered before the Handlers() dispatch
    env_ident = envelope.get("identifier")
    for name, blk in sorted(predispatch.items()):
        method_meta[name] = {"handler": "HandleMessage (inline)", "legacy_op": None,
                             "accepted_spellings": [name], "dispatch": "pre-dispatch"}
        entry = {}
        if env_ident:
            raws = scan_raw_reads(blk["body"],
                                  re.escape(env_ident) + r"\s*(?:\.|->)?\s*",
                                  blk["body_offset"], clean, "HandleMessage")
            for w in raws:
                claimed_lines.update(w["evidence_lines"])
                preds = scan_type_predicates(blk["body"], w["key"])
                # A `contains`/`at` test is a guard. A bare nlohmann `value()`
                # is NOT: it supplies its own default and tests nothing, so
                # recording it as a guard would put a defaulting read in the
                # same class as a guarded one -- the exact conflation this
                # table exists to prevent.
                gate = [k for k in w["kinds"] if k in ("contains", "at")]
                entry[w["key"]] = _record(
                    w["key"], "raw_json",
                    {"value": w["default_value"] if w["default_explicit"] else None,
                     "explicit": w["default_explicit"],
                     "source": "hand_rolled_read in HandleMessage"},
                    ({"expr": " && ".join(gate) + f'("{w["key"]}")',
                      "line": w["line"], "in_function": "HandleMessage",
                      "negated_early_bail": False, "guard_checks_type": bool(preds)}
                     if gate else None),
                    {"json_types": preds or ["any"],
                     "other_json_types_yield": "default"},
                    [{"accessor": "raw_json", "in_function": "HandleMessage", "line": ln}
                     for ln in w["evidence_lines"]],
                )
        else:
            entry["__unparsed__"] = _record(
                "__unparsed__", None, None, None, None, [],
                {"unparsed": True,
                 "reason": "pre-dispatch method found but the params identifier "
                           "could not be resolved from the envelope"})
            unparsed_entries.append({"method": name,
                                     "reason": "envelope params identifier unresolved"})
        table[name] = entry

    # ---- population census over the WHOLE file
    all_value, all_guard, all_raw = [], [], []
    for r, g, w in per_fn.values():
        all_value.extend(r)
        all_guard.extend(g)
        all_raw.extend(w)
    predispatch_raw = sum(
        1 for m in table
        if method_meta.get(m, {}).get("dispatch") == "pre-dispatch"
        for _ in table[m]
    )

    counts = {
        "methods": len(table),
        "methods_via_handlers_table": len(handlers),
        "methods_pre_dispatch": len(predispatch),
        "methods_parsed": len(table) - len(unresolved_handlers),
        "methods_with_no_parameters": sum(1 for v in table.values() if not v),
        "distinct_keys": len({k for v in table.values() for k in v}),
        "accessor_value_read_sites": len(all_value),
        "accessor_guard_sites": len(all_guard),
        "raw_read_keys_in_handlers": len(all_raw),
        "raw_read_keys_pre_dispatch": predispatch_raw,
    }
    counts["parameter_read_sites_total"] = (
        counts["accessor_value_read_sites"] + counts["raw_read_keys_in_handlers"]
        + counts["raw_read_keys_pre_dispatch"])

    return {
        "schema": "oracle/legacy-accept-table/v1",
        "generated_by": "tools/legacy_accept_table.py",
        "description": (
            "Per-method, per-key accept-table for the legacy C++ control server. "
            "The server validates almost no parameter: reads default silently. "
            "DESCRIPTIVE ONLY -- oracle-old is reference-only and is not patched."),
        "envelope": envelope,
        "accessors": accessors,
        "method_meta": method_meta,
        "counts": counts,
        "coverage": {
            "unparsed_entries": unparsed_entries,
            "unresolved_handlers": unresolved_handlers,
        },
        "methods": table,
        "_claimed_lines": sorted(claimed_lines),
    }


# ---------------------------------------------------------------------------
# Cross-check -- a SECOND, independent enumeration
# ---------------------------------------------------------------------------


def crosscheck_census(source_text: str) -> dict:
    """Count parameter accesses by a different enumeration parameter.

    build_table() enumerates by ACCESSOR NAME. This function enumerates by THE
    OBJECT: it finds every identifier that carries request parameters -- both
    `const JsonObj&` bindings and the raw json the envelope extracts from the
    request -- scopes each to its own declaring block, and counts EVERY member
    access and string subscript on it, whatever the member is called.

    A read through a member the accessor-name axis has never heard of shows up
    here and nowhere else. That is the point of running both.
    """
    clean = blank_comments(source_text)
    idents: list[dict] = []

    for m in re.finditer(r"const\s+" + PARAMS_TYPE + r"\s*&\s*(\w+)", clean):
        idents.append({"ident": m.group(1), "kind": PARAMS_TYPE, "binding": "parameter",
                       "decl_offset": m.end(), "decl_line": line_of(clean, m.start())})
    for m in re.finditer(r"\bjson\s+(\w+)\s*=\s*msg\b", clean):
        idents.append({"ident": m.group(1), "kind": "raw_json_from_envelope",
                       "binding": "local",
                       "decl_offset": m.start(), "decl_line": line_of(clean, m.start())})

    # Scope each identifier to the block it is live in, so a same-named variable
    # elsewhere in the file is not miscounted as a parameter read. A PARAMETER is
    # declared inside the signature's parens, BEFORE the body brace, so its scope
    # is found by scanning forward past the parameter list; a LOCAL is scoped by
    # walking back to its enclosing brace. Getting this wrong does not fail
    # loudly -- it inflates every count by the number of declarations -- so the
    # two bindings are handled separately rather than with one heuristic.
    for d in idents:
        start = None
        if d["binding"] == "parameter":
            depth, i, n = 1, d["decl_offset"], len(clean)
            while i < n and depth > 0:
                if clean[i] == "(":
                    depth += 1
                elif clean[i] == ")":
                    depth -= 1
                i += 1
            brace = clean.find("{", i)
            start = brace if brace >= 0 else None
        else:
            depth, i = 0, d["decl_offset"]
            while i >= 0:
                if clean[i] == "}":
                    depth += 1
                elif clean[i] == "{":
                    if depth == 0:
                        start = i
                        break
                    depth -= 1
                i -= 1
        end = match_braces(clean, start) if start is not None else -1
        if start is None or end < 0:
            d["scope"] = None
            d["scope_unresolved"] = True
            continue
        d["scope"] = (start, end)

    by_offset: dict[int, dict] = {}
    unresolved = []
    for d in idents:
        if not d.get("scope"):
            unresolved.append({"ident": d["ident"], "kind": d["kind"],
                               "decl_line": d["decl_line"],
                               "reason": "could not resolve the identifier's scope; "
                                         "its accesses are NOT counted on this axis"})
            continue
        s, e = d["scope"]
        seg = clean[s:e]
        esc = re.escape(d["ident"])
        for pat, fmt in ((esc + r"\s*(?:\.|->)\s*(\w+)", "{}"),
                         (esc + r"\s*\[\s*\"([^\"]*)\"\s*\]", '["{}"]')):
            for m in re.finditer(pat, seg):
                off = s + m.start()
                # Distinct declarations must never yield overlapping scopes; if
                # they did, deduping by offset keeps the count honest instead of
                # silently multiplying it.
                by_offset[off] = {"ident": d["ident"], "kind": d["kind"],
                                  "member": fmt.format(m.group(1)),
                                  "line": line_of(clean, off)}

    accesses = [by_offset[o] for o in sorted(by_offset)]
    members: dict[str, int] = {}
    for a in accesses:
        members[a["member"]] = members.get(a["member"], 0) + 1

    return {
        "method": ("object-axis: every member access and string subscript on every "
                   "identifier that carries request parameters, scoped to the block "
                   "the identifier is live in"),
        "identifiers": [{"ident": d["ident"], "kind": d["kind"],
                         "binding": d["binding"], "decl_line": d["decl_line"],
                         "scope_resolved": bool(d.get("scope"))} for d in idents],
        "identifiers_with_unresolved_scope": unresolved,
        "accesses": accesses,
        "member_access_counts": dict(sorted(members.items())),
        "accesses_total": len(accesses),
    }


def reconcile(table: dict, census: dict) -> dict:
    """Reconcile the accessor-name enumeration against the object-axis one."""
    members = census["member_access_counts"]
    known_value = set(ACCESSOR_NAMES)
    by_axis_value = sum(v for k, v in members.items() if k in known_value)
    by_axis_guard = members.get("has", 0)

    claimed = set(table["_claimed_lines"])
    residual = [a for a in census["accesses"]
                if a["member"] not in known_value | {"has"}]
    unclaimed = [a for a in residual if a["line"] not in claimed]

    c = table["counts"]
    return {
        "axis_a_by_accessor_name": {
            "value_read_sites": c["accessor_value_read_sites"],
            "guard_sites": c["accessor_guard_sites"],
        },
        "axis_b_by_object_member": {
            "value_read_sites": by_axis_value,
            "guard_sites": by_axis_guard,
            "total_accesses": census["accesses_total"],
        },
        "value_read_delta": c["accessor_value_read_sites"] - by_axis_value,
        "guard_delta": c["accessor_guard_sites"] - by_axis_guard,
        "non_accessor_accesses": len(residual),
        "non_accessor_accesses_claimed_by_a_table_entry": len(residual) - len(unclaimed),
        "unclaimed_accesses": unclaimed,
        # An identifier whose scope could not be resolved was not measured on
        # axis B. Reporting that as agreement would render "couldn't measure"
        # as green, so it counts as a disagreement.
        "identifiers_not_measured": census["identifiers_with_unresolved_scope"],
        "agrees": (c["accessor_value_read_sites"] == by_axis_value
                   and c["accessor_guard_sites"] == by_axis_guard
                   and not unclaimed
                   and not census["identifiers_with_unresolved_scope"]),
    }


# ---------------------------------------------------------------------------
# Derived views the consumer asked about
# ---------------------------------------------------------------------------


def hazard_views(doc: dict) -> dict:
    """The classes the table exists to separate, precomputed.

    GUARDEDNESS AND DEFAULT-EXPLICITNESS ARE ORTHOGONAL and are reported on
    separate axes here. An earlier version of this view listed "unguarded reads
    with an implicit default", which is a proxy for neither property: it
    admitted guarded sites that merely lacked a default, and it excluded
    genuinely unguarded sites BY CONSTRUCTION whenever they carried one
    (`getU32("addr", 0)` in read_vram / write_vram). Filtering unguardedness by
    anything to do with defaults reintroduces that error.
    """
    unguarded, coercing, absence_only = [], [], []
    for method, keys in sorted(doc["methods"].items()):
        for key, rec in sorted(keys.items()):
            d = rec.get("default") or {}
            acc = rec.get("accessor")
            if rec.get("guarded_by") is None and acc not in (None, "raw_json"):
                unguarded.append({
                    "method": method, "key": key, "accessor": acc,
                    "default": d.get("value"),
                    "default_explicit": d.get("explicit"),
                    "partially_guarded": bool(rec.get("partially_guarded")),
                    "line": rec["read_sites"][0]["line"] if rec["read_sites"] else None,
                })
            g = rec.get("guarded_by")
            if g and g.get("guards_against") == "absence":
                absence_only.append({
                    "method": method, "key": key, "accessor": acc,
                    "guard_line": g.get("line"),
                    "malformed_value_yields": d.get("value"),
                })
            sh = rec.get("accepted_shapes") or {}
            if sh.get("other_string_values_ignore_caller_default"):
                coercing.append({
                    "method": method, "key": key, "declared_default": d.get("value"),
                    "accepted_strings": sh.get("string_values_accepted"),
                    "unlisted_string_yields": sh.get("other_string_values_yield"),
                    "inverts_declared_default": d.get("value") is True,
                })

    by_key: dict[str, list] = {}
    for u in unguarded:
        by_key.setdefault(u["key"], []).append(u["method"])

    return {
        "note": ("guardedness and default-explicitness are ORTHOGONAL; each list "
                 "below filters on exactly one of them"),
        "unguarded_reads": unguarded,
        "unguarded_reads_by_key": {k: sorted(v) for k, v in sorted(by_key.items())},
        "unguarded_reads_with_an_explicit_default":
            [u for u in unguarded if u["default_explicit"]],
        "unguarded_reads_with_an_implicit_default":
            [u for u in unguarded if not u["default_explicit"]],
        "guards_that_cover_absence_but_not_type": absence_only,
        "string_coercion_keys": coercing,
        "string_coercion_keys_that_invert_their_declared_default":
            [c for c in coercing if c["inverts_declared_default"]],
    }


def summarize(doc: dict) -> str:
    L = []
    src = doc["source"]
    L.append("legacy accept-table -- oracle-old ControlSocket.cpp")
    rev = src.get("revision")
    L.append(f"  source revision : {rev or 'UNAVAILABLE -- ' + str(src.get('revision_unavailable_reason'))}")
    L.append(f"  source dirty    : {src.get('source_file_dirty')}")
    c = doc["counts"]
    L.append(f"  methods         : {c['methods']} "
             f"({c['methods_via_handlers_table']} via Handlers(), "
             f"{c['methods_pre_dispatch']} pre-dispatch, "
             f"{c['methods_with_no_parameters']} take no parameters)")
    L.append(f"  distinct keys   : {c['distinct_keys']}")
    L.append(f"  parameter reads : {c['parameter_read_sites_total']} "
             f"({c['accessor_value_read_sites']} accessor "
             f"+ {c['raw_read_keys_in_handlers']} raw in handlers "
             f"+ {c['raw_read_keys_pre_dispatch']} raw pre-dispatch)")
    L.append(f"  guard sites     : {c['accessor_guard_sites']}")

    x = doc["crosscheck"]["reconciliation"]
    L.append("  cross-check     : " + ("AGREES" if x["agrees"] else "DISAGREES"))
    L.append(f"    axis A (accessor name)  value={x['axis_a_by_accessor_name']['value_read_sites']} "
             f"guard={x['axis_a_by_accessor_name']['guard_sites']}")
    L.append(f"    axis B (object member)  value={x['axis_b_by_object_member']['value_read_sites']} "
             f"guard={x['axis_b_by_object_member']['guard_sites']} "
             f"total_accesses={x['axis_b_by_object_member']['total_accesses']}")
    L.append(f"    non-accessor accesses   {x['non_accessor_accesses']} seen, "
             f"{x['non_accessor_accesses_claimed_by_a_table_entry']} claimed by a table entry")
    for u in x["unclaimed_accesses"]:
        L.append(f"    UNCLAIMED {u}")

    cov = doc["coverage"]
    if not cov["unparsed_entries"] and not cov["unresolved_handlers"] and x["agrees"]:
        L.append("  parse complete  : yes")
    else:
        L.append("  parse complete  : NO -- the table below is INCOMPLETE")
        for u in cov["unparsed_entries"]:
            L.append(f"    UNPARSED {u}")
        for u in cov["unresolved_handlers"]:
            L.append(f"    UNRESOLVED HANDLER {u}")

    env = doc["envelope"]
    L.append(f"\n  envelope        : "
             + (f"a `{env['envelope_member']}` that is absent or not "
                f"{env['required_shape']} becomes {env['substituted_when_absent_or_wrong_shape']!r} "
                f"(line {env['line']}) -- every key then reads its default"
                if env.get("parsed") else f"UNPARSED: {env.get('reason')}"))

    hv = doc["hazards"]
    ug = hv["unguarded_reads"]
    L.append(f"\n  UNGUARDED reads : {len(ug)} total "
             f"({len(hv['unguarded_reads_with_an_explicit_default'])} carry an explicit "
             f"default, {len(hv['unguarded_reads_with_an_implicit_default'])} do not)")
    L.append("    guardedness and default-explicitness are ORTHOGONAL; filtering one "
             "by the other is how both false positives and")
    L.append("    false negatives get in, so the full unguarded set is listed by key:")
    for k, methods in hv["unguarded_reads_by_key"].items():
        L.append(f"    {k:22s} {', '.join(m.split('/')[-1] for m in methods)}")

    ao = hv["guards_that_cover_absence_but_not_type"]
    L.append(f"\n  guards covering ABSENCE but not TYPE : {len(ao)}")
    L.append("    has() is satisfied by any present, non-null value, so a malformed "
             "value passes the guard and still reads the default.")

    sc = hv["string_coercion_keys"]
    inv = hv["string_coercion_keys_that_invert_their_declared_default"]
    L.append(f"\n  string-coercion keys : {len(sc)} (an unlisted string returns a hard "
             f"false -- NOT the declared default); {len(inv)} INVERT a true default")
    for s in sc:
        flag = "  <-- INVERTS" if s["inverts_declared_default"] else ""
        L.append(f"    {s['method']:32s} {s['key']:20s} "
                 f"declared={s['declared_default']!r}{flag}")
    return "\n".join(L)


def generate(source_text: str, src_meta: dict) -> dict:
    doc = build_table(source_text)
    census = crosscheck_census(source_text)
    doc["source"] = src_meta
    doc["crosscheck"] = {"census": census, "reconciliation": reconcile(doc, census)}
    doc["hazards"] = hazard_views(doc)
    doc["coverage"]["complete"] = (
        not doc["coverage"]["unparsed_entries"]
        and not doc["coverage"]["unresolved_handlers"]
        and doc["crosscheck"]["reconciliation"]["agrees"])
    return doc


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--source", help="path to the oracle-old checkout")
    ap.add_argument("--source-file", help="parse this file directly (testing)")
    ap.add_argument("--out", help="write the output here (default: stdout)")
    ap.add_argument("--format", choices=("json", "summary"), default="json")
    ap.add_argument("--fail-on-gap", action="store_true",
                    help="exit 1 if coverage is incomplete or the cross-check disagrees")
    args = ap.parse_args(argv)

    if args.source_file:
        path = Path(args.source_file)
        src_meta = {"repo": None, "file": str(path), "revision": None,
                    "revision_unavailable_reason":
                        "--source-file bypasses revision discovery (test mode)"}
    else:
        try:
            root = find_oracle_old(args.source)
        except FileNotFoundError as exc:
            print(f"error: {exc}", file=sys.stderr)
            return 2
        path = root / SOURCE_RELPATH
        src_meta = source_revision(root)

    try:
        text = path.read_text(encoding="utf-8", errors="surrogateescape")
    except OSError as exc:
        print(f"error: cannot read {path}: {exc}", file=sys.stderr)
        return 2

    doc = generate(text, src_meta)

    if src_meta.get("revision") is None:
        print("warning: source revision UNAVAILABLE -- this table cannot be pinned "
              f"({src_meta.get('revision_unavailable_reason')})", file=sys.stderr)
    if doc["coverage"]["unparsed_entries"] or doc["coverage"]["unresolved_handlers"]:
        print("warning: parse INCOMPLETE -- unparsed entries are present; do not "
              "read this table as a complete list", file=sys.stderr)
    if not doc["crosscheck"]["reconciliation"]["agrees"]:
        print("warning: the two independent enumerations DISAGREE -- see "
              "crosscheck.reconciliation", file=sys.stderr)

    out = (json.dumps(doc, indent=2, sort_keys=False) + "\n"
           if args.format == "json" else summarize(doc) + "\n")
    if args.out:
        Path(args.out).write_text(out, encoding="utf-8")
    else:
        sys.stdout.write(out)

    if args.fail_on_gap and not doc["coverage"]["complete"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
