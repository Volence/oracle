# The legacy accept-table

**Tool:** `tools/legacy_accept_table.py`
**Tests:** `tools/test_legacy_accept_table.py` (48)
**Runner:** `tools/run_accept_table_tests.sh`
**Derived from:** `oracle-old/linux-port/gui/ControlSocket.cpp` @ `58b6f81`

`oracle-old` is **reference-only**. This tool reads it and never writes it.
Describing the hazard is the deliverable; repairing it is not — the cutover
exists to delete that server.

## What this is for

The aeon lane reaches the legacy C++ control server through Python probe
tools. They asked for a machine-derived table of what that server actually
accepts, so their gate pins against our source instead of a hand-written list
that rots. The tool emits exactly that:

```
{method: {key: {accessor, default, guarded_by, accepted_shapes}}}
```

Per **method**, per **key**. A flat key vocabulary is insufficient: a key valid
on one method and absent on another is precisely the case a flat list cannot
catch.

## Running it

```bash
python3 tools/legacy_accept_table.py --out accept-table.json   # the table
python3 tools/legacy_accept_table.py --format summary          # human view
python3 tools/legacy_accept_table.py --fail-on-gap             # exit 1 on gaps
tools/run_accept_table_tests.sh                                # tests + table
```

`--source DIR` names the `oracle-old` checkout; otherwise `$ORACLE_OLD`, then
an upward walk for a sibling `oracle-old`. An **explicitly named** source that
does not contain the target is an error, never a fallback to another tree.

The output records the `oracle-old` revision it derived from. A table that
does not name the revision it was true at degrades into a wrong instruction
rather than a historical note, so an unavailable revision is reported as
`null` with a reason — never as an empty string a consumer could pin against.

## What the server actually does

Nearly every parameter read goes through one of four defaulting accessors on
`JsonObj`, each shaped `if (!has(k)) return d;`. There is **no unknown-key
rejection anywhere**. A misspelled key, a wrong-typed value, or an
unrecognised string does not produce an error — it produces a silently
substituted default, and the call returns success.

At `58b6f81`: **55 methods** (53 dispatched through `Handlers()`, 2 answered
before it), **42 distinct keys**, **67 parameter reads** (63 accessor calls,
1 hand-rolled raw read in a handler, 3 more pre-dispatch), 15 `has()` guards,
20 methods that take no parameters at all.

### The two hazard classes the table exists to separate

**1. Unguarded reads.** 43 of them. An unguarded memory-path `addr` and a
`has()`-guarded `enabled` differ ONLY in whether a guard sits above the read;
a table carrying `default` but not `guarded_by` renders those two classes
identically and is actively misleading.

The unguarded address reads are:

| method | key | accessor | default |
|---|---|---|---|
| `emulator/read_vram` | `addr` | `getU32` | `0` (explicit) |
| `emulator/write_vram` | `addr` | `getU32` | `0` (explicit) |
| `emulator/z80_read` | `addr` | `getU32` | `0` (implicit) |
| `emulator/z80_write` | `addr` | `getU32` | `0` (implicit) |

**Guardedness and default-explicitness are orthogonal.** Two of those four
carry an explicit default and two do not, and there are guarded reads in both
states as well. Filtering one property by the other produces false positives
*and* excludes real unguarded sites by construction. `guarded_by` is therefore
computed from enclosing block structure alone:

- `block` — `if (has(k)) { … }`, protects that block only
- `early_bail` — `if (!has(k)) return …;`, protects everything after it
- `transitive` — the guard lives in another function of the call chain

A negated `has()` that does not leave the function protects nothing; a `has()`
in an unrelated branch protects nothing outside it. A key guarded on only some
of its sites summarises as **unguarded**, with `partially_guarded` set — one
unguarded path is the path a caller has to plan for.

**2. Values that are accepted but not honoured.** A gate checking key
*spelling* passes all of these, because the keys are spelled correctly.

`getBool` accepts a boolean, any number, or one of the strings `"true"`,
`"1"`, `"yes"`. **Any other string returns a hard `false` — not the declared
default.** At the five sites declaring `true`, a correctly-spelled, guarded
key silently inverts:

| method | key | declared | `{"…":"on"}` reads |
|---|---|---|---|
| `emulator/reset` | `run` | `true` | `false` |
| `emulator/reload_rom` | `reset` | `true` | `false` |
| `emulator/reload_rom` | `wait` | `true` | `false` |
| `emulator/watchpoint_add` | `write` | `true` | `false` |
| `emulator/hold` | `down` | `true` | `false` |

The same coercion bites at the `false`-defaulting sites too, where it silently
does the opposite of what was asked rather than inverting a stated default:

- `set_layer_enabled` and `set_channel_enabled` compute `*flag = !on`
  (lines 1527 and 1571 — `flag` holds *mute*), so `{"enabled":"on"}` **mutes
  the layer the caller asked to enable**.
- `set_profiler` assigns `on` directly (line 1927), so the same request
  **disables** the profiler the caller asked to enable.

### Guards cover absence, never type

`has()` is satisfied by any present, non-null value — it applies no type test
to the value at the key. `getInt`'s string path throws inside `stoll` and is
swallowed by `catch (...)`, so `{"addr":"0xZZZZ"}` resolves to the default at
**every** site, including the guarded ones. Each guard record carries
`guards_against: "absence"` rather than leaving that to be inferred.

Worse than rejection: `stoll` is called with a `nullptr` parse-position
out-param and nothing checks full consumption, so trailing garbage is not
rejected either. `{"len":"12abc"}` reads **12** — neither the intended value
nor the default. `accepted_shapes` carries this as
`trailing_garbage_rejected: false`.

### The envelope defaults too

`params` that is absent or not an object is replaced wholesale with `{}`
(line 2873), after which every key on every method reads its default. That is
a whole-request behaviour a per-key table would otherwise miss, so it is
recorded separately under `envelope`.

### The non-accessor reads

- `ParseButtons` reads `buttons` directly off the raw JSON behind
  `contains` + `is_array` + per-element `is_string`. The checks are
  type-aware but **not error-reporting**: a misspelled `buttons` yields an
  empty vector, the press does nothing, and the call returns success.
- `initialize` and `initialized` are answered **before** the `Handlers()`
  dispatch and are therefore absent from any table derived from `Handlers()`
  alone. `initialize` reads `protocolVersion`, `clientCapabilities`, and the
  nested `clientCapabilities.events`.

### Where a bad value *is* caught

"Validates no parameter" is about the accessors, not about the whole server.
Four sites reject a bad **value** downstream of the read, all by a lookup or
a parse failing rather than by any type check:

| site | rejects |
|---|---|
| `set_layer_enabled` (1525) | `layer` not in `plane_a/plane_b/window/sprites` |
| `set_channel_enabled` (1569) | `channel` not in `fm1..fm6/dac/psg1..psg3/psg_noise` |
| `audio_spectrum` (1756) | `source` not in `fm/psg` |
| `write_vram` (2145) | `bytes` not valid hex |

plus `initialize`'s `protocolVersion` mismatch. This matters for reading the
unguarded list: `layer`, `channel` and `source` are unguarded reads whose
defaulted `""` then fails a lookup and produces an error, so they are
unguarded-but-validated. The unguarded `addr` and `len` reads on the memory
paths have no such downstream check — a defaulted `0` is a perfectly valid
address.

## How the parse is checked

Two independent enumerations run on every invocation, and they enumerate by
genuinely different parameters:

- **Axis A** enumerates by **accessor name** (`get`/`getInt`/`getU32`/
  `getBool`) plus the raw-read family, and builds the table.
- **Axis B** enumerates by **the object**: every member access and string
  subscript on every identifier carrying request parameters, scoped to the
  block that identifier is live in, *whatever the member is called*.

Axis B is not a restatement of axis A. A read through a member axis A has
never heard of appears on axis B and nowhere else. At `58b6f81` both report
63 value reads and 15 guards, and all 10 non-accessor accesses are claimed by
a table entry. Anything unclaimed is reported as a gap; an identifier whose
scope cannot be resolved counts as a **disagreement**, never as green.

Axis B earned its cost immediately: it is what surfaced the two pre-dispatch
methods that a `Handlers()`-derived table omits entirely.

**Unparsed is never silent.** A handler whose body cannot be found, or a raw
read whose key is not a string literal, appears in the table marked
`unparsed` with a reason, is listed under `coverage`, and warns on stderr. A
method that genuinely takes no parameters appears as present-and-empty. Those
are different claims and the output distinguishes them, because a consumer
would otherwise read a short table as a complete one.

## Verification

48 tests, run by `tools/run_accept_table_tests.sh`, which also regenerates the
table with `--fail-on-gap` so a parse that has quietly stopped covering the
source fails there rather than shipping a short table.

Tests split deliberately. **Fixture** tests parse a synthetic C++ file whose
accessors are given *different* semantics from the real ones — a coercion set
of `{"on","off"}`, `$` as the only hex prefix, octal radix — so a tool that
transcribed the real file instead of parsing it fails. **Source** tests assert
only properties re-derived at test time by a different route than the parser
used; the load-bearing one demands that every line number the table reports
actually contains the key it claims, checked against the raw file.

Every test was proven red-first by a 19-mutation sweep. Two rounds of that
sweep each caught one **vacuous** gate in this suite — a test asserting on its
own fixture input, and an untested partial-guard summary. Both are fixed and
both now go red.

## Not done, deliberately

- **Not wired into CI.** The consumer validates the output against their own
  independent read *before* it becomes a gate. Once an unvalidated instrument
  is a gate, every failure it produces presents as the consumer being broken.
- **Nothing in `oracle-old` was touched.**
- **No runtime confirmation.** Every claim here is derived statically from
  source. The behavioural readings — `{"enabled":"on"}` reads false,
  `{"len":"12abc"}` reads 12, a non-object `params` defaults every key — are
  sound from the code but have **not** been confirmed against a running
  server. Tagged for foreground follow-up if anyone wants them observed.
