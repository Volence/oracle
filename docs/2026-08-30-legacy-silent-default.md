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
