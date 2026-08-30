# The legacy server defaults every missing parameter, and six sites default to address zero

**Date:** 2026-08-30 · **Lane:** oracle · **Status:** measured, not fixed. No code changed.
**Subject repo:** `oracle-old` (the legacy C++ Exodus port), file
`linux-port/gui/ControlSocket.cpp`, read at the working tree on this box.
**Why it matters today:** the `mcp__oracle__*` surface **still reaches this server**, so every lane
in the suite that debugs through MCP is on this path.

## Provenance — this started as a peer's claim and is banked only because it was checked

aeon raised it in passing while acknowledging a different signal, as an aside explaining why *their*
Rider 3 ruling exists: *"the legacy C++ path reads its parameter with a tolerant `getInt(key, 30000)`
— so a well-meaning rename against a legacy-seam probe does not go red, it silently replaces every
hand-chosen budget with 30 s."* That is a claim about **our** tree arriving in **mail**, which is
protocol bar 20's exact shape, so it was verified here rather than banked.

**Their claim is confirmed and understates the exposure.** They named one site and one key. The
mechanism is the accessor, and it is 34 call sites wide.

## ⚑ CORRECTION, same day, and it is this document's own bar turned on itself

**The family is FOUR accessors and 63 call sites, not one accessor and 34.** The original text below
counted `getInt` and `getU32` and reported "34 call sites". That number is accurate for what it
counted and **understates the mechanism it was offered as evidence for**, which is the sentence
*"validates no parameter at all"*.

| accessor | line | silent default | call sites |
|---|---|---|---|
| `get` (string) | `:119` | `""` | 18 |
| `getInt` | `:130` | `0` | 23 |
| `getU32` | `:152` | `0` | 11 |
| `getBool` | `:156` | `false` | 11 |

**63 total.** Every one has the same `if (!has(k)) return d;` shape.

**How it was found, because the method is the point.** aeon cross-checked this finding and, in doing
so, enumerated **every** `get*("key")` in the file — a wider alphabet than the `getInt|getU32` this
document used. They were not auditing my enumeration; they were building their own consumer-side
vocabulary check and needed the full set. That is protocol bar 19 exactly: **what makes two
derivations independent is the enumeration parameter, and mine was too narrow.** My pass agreed with
itself and would have agreed with itself any number of times, because *"I ran it twice"* and *"I ran
it twice the same way"* produce identical output. It is also bar 21's shape — the discriminator fired
by accident, as a by-product of someone doing their own work, not because anyone invoked it.

**What does NOT change:** the six unguarded memory-path sites, the headline, and the absence of any
unknown-key rejection. aeon re-read the file independently and confirmed those line for line
(`:348`, `:702`, `:726`, `:782` for `addr`; `:615`, `:739` for `value`). The correction widens the
mechanism; it does not touch the consequence.

**Consumer side, measured by aeon and theirs to own** (aeon `c584df3d`, pushed): their three
legacy-seam tools make 14 memory-path calls spelling `addr`, `value`, `len`, `bytes` and `width`, all
correct. **No incident.** Their own caveat, kept because it is the right one: a clean grep is not a
gate, since the seam is structurally incapable of reporting a regression — the next misspelling lands
silently and the tool keeps returning success. They have booked deriving the vocabulary from this
server's source and pinning every send site against it.

*Original text follows unedited, per this repo's supersession rule — a reader meeting the "34" cold
needs to see that it was superseded, not that it was never written.*

## ⚑ SECOND CORRECTION — the population is 64 and is now CLOSED, and one route inverts intent

**aeon varied the enumeration parameter deliberately** (by *how the params object is touched at all*,
rather than by accessor name) and found a 64th parameter read outside the accessor family:
`ParseButtons`, `:1576-1583`, reading `(*req.p)["buttons"]` directly behind `contains` + `is_array` +
per-element `is_string`. **Verified here line for line.** It is the only parameter read in the file
that type-checks its input, and it is *not* a counterexample to the headline: a misspelled `buttons`
fails `contains` and yields an empty vector, so a `press` with a mistyped key **presses nothing and
returns success** — arguably the worst of the set, since it is indistinguishable from a ROM that
ignored the input.

**Their open question is now closed, and the answer is the good one.** They flagged that
`ParseButtons` was *"presumably not the only helper of its shape"* and that they had swept only the
params object, not helpers generally. Swept here by a third parameter — every function that receives
the params object at all:

* **59** signatures take `const JsonObj&`; **58** are helpers/handlers.
* Every direct touch of the raw `json` in the entire file: `:117` (`has`), `:122` (`get`), `:133`
  (`getInt`), `:159` (`getBool`) — i.e. **inside the four accessors** — plus `:1579-1580`
  (`ParseButtons`). Nothing else.

`JsonObj` exposes exactly one member (`p`) and the four accessors, so those two greps between them
cover every route into the params object. **`ParseButtons` IS the only helper of its shape. The
population is 63 accessor sites + 1 direct read = 64, and it is a complete enumeration rather than a
running total.**

### A correction against my own first suspicion, kept because the near-miss is the lesson

Three `getBool("enabled")` sites pass no explicit default (`:1526`, `:1570`, `:1926`) and two of them
feed `*flag = !on`. That looks like a silent **inversion**, and I was about to report it as one.
**It is not, for the missing-key case:** all three are guarded by an explicit
`if (!req.has("enabled")) return ErrorReply(...)` one or two lines above. Reading the lines around the
cited line is what caught it — protocol bar 11, on my own finding.

**But the guard covers absence, not type**, and that gap is real. `has()` (`:117`) is satisfied by any
present non-null value, and `getBool` (`:156`) accepts only `"true"`, `"1"`, `"yes"` from a string and
falls through to `d` for everything else. So:

```
{"layer":"plane_a","enabled":"on"}     -> passes the guard, reads FALSE, executes *flag = !false
                                       -> PLANE A IS MUTED when the caller asked to enable it
```

`"True"`, `"TRUE"`, `"enabled"`, an array or an object all do the same. **Mitigation, stated so this
is not over-claimed:** the reply echoes its own decision (`addBool("enabled", on)`), so a caller that
reads the echo can detect it. This is the one place in the file where the server tells you what it
decided — it is a partial mitigation, not a validation, and it only helps a caller who checks.

## The mechanism

`ControlSocket.cpp:130`:

```cpp
long long getInt(const std::string& k, long long d = 0) const
{
    if (!has(k)) return d;
    ...
    catch (...) { return d; }
    return d;
}
uint32_t getU32(const std::string& k, uint32_t d = 0) const { return (uint32_t)getInt(k, (long long)d); }
```

Three separate paths return the default and **none of them can report that it happened**: the key is
absent, the string does not parse (`catch (...)`), or the JSON value is of an unhandled type. The
parameter `d` itself defaults to `0`, so a call site that omits a default gets **zero**, not an error.

There is **no unknown-key rejection anywhere in the file** — verified as a genuine absence under a
positive control (`grep -qEi 'unknown (param|key|propert)|unevaluated|additionalProp'` exits 1; a
control pattern that must exist exits 0), rather than read off empty output.

## The enumeration

34 `req.getInt` / `req.getU32` call sites. **Eight** pass no explicit default; **two of those eight
are guarded** by `req.has(...)` on the same line (`:1954` `frames`, `:1959` `top`) and are safe.

**Six unguarded silent-zero sites, and they are the worst six in the file** — every one is an address
or a value on a memory path:

| line | call | what a misspelled or unparseable key does |
|---|---|---|
| `:348` | `req.getU32("addr")` | resolves to **address 0** |
| `:615` | `req.getInt("value")` | writes **0** |
| `:702` | `req.getU32("addr")` | address 0 |
| `:726` | `req.getU32("addr")` | address 0 |
| `:739` | `req.getInt("value")` | byte **0** |
| `:782` | `req.getU32("addr")` | address 0 |

So a client that misspells `addr` on a legacy-server memory write does not get an error. It writes to
address zero and receives a success reply. `addr: "0xZZZZ"` does the same thing through the `catch`.

The site aeon named is real and is one of the 26 with an explicit default:
`:894` `const double timeoutSec = (double)req.getInt("timeout_ms", 30000) / 1000.0;`

## Why this is protocol bar 15, exactly

Bar 15: *when two implementations of one contract disagree about unknown keys, sequence the cutover so
it lands on the STRICT one — a permissive implementation can only ever report success.* This is that
bar's precedent, now measured in our own tree rather than argued. `oracle-aether` declares
`unevaluatedProperties: false` and refuses an unknown key `-32602`, loudly, at the caller's own call
site. The legacy server cannot refuse anything.

**The consequence for the cutover is that it is not merely a performance or feature migration.** Every
consumer still on the legacy seam is running without parameter validation of any kind, and the failure
mode is silent and points at the ROM rather than at the caller — the same shape as the breakpoint gap
closed at `4501c8b`, one layer down and much wider.

## What is NOT claimed here

- **Not that anyone has been bitten.** No consumer was audited for a misspelled key; this is the
  exposure, not an incident. Finding one would mean grepping the consumer trees, which is bar 14's
  enumeration and was not run.
- **Not a fix recommendation for `oracle-old`.** It is reference-only and being retired; hardening it
  would be work spent on the thing the cutover exists to delete.
- **Not a claim about aeon's tree.** Their probe sites and their Rider 3 ruling are their measurement,
  relayed, and are cited as theirs.

## The actionable half

Owner ruling 4 (2026-08-22) requires our README to say plainly that the MCP surface still reaches the
legacy C++ server. **This finding is what that sentence should say next to it**: not merely that the
surface is legacy, but that the legacy server validates no parameter and defaults six memory-path
sites to zero. That is the fact most likely to mislead a reader, stated at its real cost.
