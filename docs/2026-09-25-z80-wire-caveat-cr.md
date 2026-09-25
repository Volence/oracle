A PROPOSAL. Nothing here is ruled. It recommends that oracle retire `F-Z80-WIRE-CAVEAT-ABSENT`. No contract or schema change is proposed.

# F-Z80-WIRE-CAVEAT-ABSENT: the Z80 watch caveat on the wire, re-derived

**Kind:** investigation
**Date:** 2026-09-25
**Oracle base:** `012cfd9`. The tap landed at merge `6e84441`.
**Contract read:** empyrean `origin/main` at `9540ecc9`, `contract/protocol.md`. It differs from `616c2026` (the §11.52
text oracle vendored against) only by the 9-line §11.52 REGISTERED paragraph at `protocol.md:6280-6287`.
**Schema:** the vendored `crates/oracle-aether/tests/contract/bus-protocol.schema.json` is blob `0fa047c9`. empyrean
`origin/main:contract/schema/bus-protocol.schema.json` is the same blob, so the schemas **do not differ**.
**Scope:** docs only. No emulator was run, and no `mcp__oracle__*` tool was used.

## 1. Verdict: case (C), retire the row

The four watch fragments **do** declare `caveat`, so a caveat here would not be a conformance failure. That rules out
case (B). Case (A), building the caveat, is still the wrong call. §11.52 put the permanent limit into §6 as normative
prose. A per-reply caveat for the same limit would fire on nearly every bus watch a client arms, and §2.4's advisory
names that as the pathology to avoid. §6 has already turned down a per-reply caveat once, for a VDP hit's standing
`mclk` granularity, on the same ground. So the right move is to retire the row, not to deliver it.

## 2. The premise, re-derived

1. **Core has the caveat.** `Watchpoints::caveats()` is at `crates/oracle-core/src/watchpoints.rs:870`. Its Z80 branch
   (`:900-921`) fires for any bus watch that `z80_shaped_could_match` says overlaps `Z80_REACHABLE_68K`
   (`:943-947`). Those ranges are:
   - cartridge `$000000-$3FFFFF`;
   - Z80 RAM, including its 68000 alias, `$A00000-$A0FFFF`;
   - work RAM `$E00000-$FFFFFF`.

   The ranges are derived from `z80::bus::Z80Bus` and pinned page by page by
   `tests::caveat_one_covers_exactly_the_pages_the_z80_bus_window_resolves` (`:2146`). The only filters are a function
   code, a word or long `size`, or a parity that cannot match (`:959-966`). **Direction is not a filter.** A write-only
   watch still draws the caveat, and that is correct: the Z80's bank-window *writes* to work RAM are not delivered
   either.
2. **The wire does not carry it.** `grep -rn 'caveats(' crates/oracle-aether/src` returns 0 lines. `watchpoint_hits`
   (`crates/oracle-aether/src/engine.rs:8688`) inserts `hits`, `total`, `returned`, `limit`, `truncated`, `cursor`,
   `dropped`, `seen` and `matched`, and never `caveat`. The only reader is the player's watch tab,
   `crates/oracle-player/src/stopping.rs:717` (`caveats: w.caveats()`).
3. **The fragments declare it.** All four watch methods have `result.properties.caveat` (a string):
   `watchpoint_add`, `watchpoint_clear`, `watchpoint_list` and `watchpoint_hits`. The `watchpoint_hits` fragment is at
   schema `:4329`, and its `caveat` description at `:4624` reads: *"Every machine-actionable warning this instrument
   can raise already has a typed key — `seen`, `keysCapped`, `censusOverflow` — and the one permanent property (a VDP
   hit's step-granular `mclk`) is stated in §6 instead, because §2.4's advisory is that an always-present caveat is one
   clients learn to ignore."* §6 says the same about the rows (`protocol.md:1349-1350`): *"every one of these methods
   may emit one and every fragment declares it."*
4. **§6 now states the limit as a permanent property of the method** (`protocol.md:1433-1437`):
   > A `bus` watch observes the 68000's accesses. Of the Z80's accesses, a server offers a watch **exactly one kind:
   > the Z80's writes to the YM2612 and SN76489 registers.** […] They do not match, and `seen` does not count them.
   > **So a zero on an address the Z80 can reach says nothing about the Z80.** The 68000 alias of Z80 RAM
   > (`$A00000–$A0FFFF`), cartridge ROM and work RAM are all such addresses.

   That paragraph did not exist when the row was booked (`docs/2026-09-25-z80-watch-cr.md:578`, before the ruling).
   At booking time a caveat was the only disclosure there could be. After §11.52 it is not.

## 3. Why a conditional caveat still counts as the constant-caveat pathology here

- **§2.4's advisory** (`protocol.md:626-634`): *"A caveat that is always present is documentation wearing signal's
  clothes. […] Servers SHOULD prefer conditional caveats, emitted when the specific weakness is actually present,
  and put a permanent property of a method in this document instead, where it is read once by an implementer rather
  than ignored forever by a client."* The shape it points to is `screenshot`. That caveat fires on a **per-reply
  fact**: this particular picture came from a state render.
- **This condition is a property of the watch's range, not of the reply.** A work-RAM watch carries it on every
  `watchpoint_hits` reply for its whole life. The watch's range never changes, and the Z80's non-delivery is permanent
  until the deferred (D)/(F) options (§11.52: *"(D)/(F), the wider Z80 stream, are DEFERRED"*).
- **It fires on the commonest watch.** Core's own comment says so (`watchpoints.rs:900-902`): *"the commonest watch is
  on work RAM and draws caveat 1"*. The ranges that escape it are the unmapped gaps, I/O at `$A10000+` and the VDP
  ports. The caveat would be present on almost every reply a client actually reads, which is what §2.4 means by
  *always*.
- **§6 has already made this exact ruling for a sibling limit** (`protocol.md:1465-1468`): the VDP hit's
  instruction-granular `mclk` *"is a standing property of a VDP-space hit and is stated here, once, rather than
  repeated as a per-reply `caveat` — §2.4's advisory: a caveat that is always present is one clients learn to
  ignore"*. The Z80 limit is also a standing property of a class of ranges, and it is also now stated once in §6.
- **The reply is not weaker than its shape suggests.** §2.4 defines a caveat as a statement that *"the reply is less
  trustworthy, less complete or less direct than its shape suggests"* (`protocol.md:604`). Since §11.52, the contract
  defines the shape as *"A `bus` watch observes the 68000's accesses"*, with the Z80's YM/PSG writes as the one
  exception. A reply that leaves out the Z80's RAM traffic is exactly as complete as its contract shape says.
- **Nothing a client must act on is missing.** Under §2.4 rule 3 an actionable consequence needs a typed key, and a
  caveat cannot carry one. What a client does with this fact (not trusting a zero for Z80 behaviour) is fixed by the
  address alone. It can learn that from §6 once, with no reply involved.

**The narrower condition was checked too.** The earlier booking proposed firing only *"in a run where the Z80
executed"* (`docs/2026-09-25-z80-watch-cr.md:578-579`). On every consumer this lane serves, the Z80 runs a sound
driver in every frame after boot (aeon's own witnesses rely on that, per §11.52's consumer note), so that narrowing
removes almost no firings. This is reasoned, not measured, because no machine was run. It is tagged below.

## 4. Options

| | Option | Wire change | Cost | Assessment |
|---|---|---|---|---|
| **A1** | Wire the Z80 branch of `caveats()` into the `watchpoint_hits` reply (and `add`/`list`) when any watch overlaps `Z80_REACHABLE_68K` | `caveat` present on nearly every bus-watch reply | S. About 20 lines in `engine.rs`, plus gates | Conformant, since the key is declared. It fails §2.4's SHOULD and contradicts the §6 VDP precedent. **Not recommended.** |
| **A2** | A1, gated on "the Z80 executed while the watch was armed" | As A1 in practice | M. Needs a new Z80-activity latch in core, which is a hot-path touch | The same pathology, at a higher price. **Not recommended.** |
| **A3** | Wire `caveats()` whole | Adds the VDP-granularity, `seen = 0` and census-cap strings too | S | **Refused by the contract.** The VDP line is the caveat §6 `:1465-1468` explicitly declines. `seen` and `keysCapped` are typed keys (schema `:4624`). |
| **C** | **Retire the row.** §6 `:1433-1437` is the disclosure. The player keeps its caveat, because a GUI panel that a human reads per watch is not §2.4's wire | None | None | **Recommended.** |
| B | Declare `caveat` in the fragment | n/a | n/a | **Moot.** It is already declared. |

**What (C) leaves in place, on purpose (three-surface parity: this is the decision a gap needs).** The player's watch
tab keeps showing the Z80 line (`stopping.rs:717`). That surface is a human looking at one watch. It is not a client
parsing a stream of replies, and §2.4's advisory is about the stream. The MCP and plain-Aether surfaces carry no
caveat and rely on §6. That asymmetry is deliberate, and this document records it.

## 5. Recommendation and exact text

**Retire `F-Z80-WIRE-CAVEAT-ABSENT`** with the reason *"§11.52 put the Z80 limit into §6 as normative prose
(`protocol.md:1433-1437`). A per-reply caveat for a standing per-range property is the one §6 `:1465-1468` declines
for VDP `mclk`, and it would fire on every work-RAM watch."*

- **Contract:** no change proposed. §6 and the schema are already right.
- **Schema:** no change proposed. The `caveat` stays declared on all four fragments. It still permits a future
  conditional caveat for a genuinely per-reply weakness.
- **Oracle `docs/lane-status.json`:** the overseer closes the `F-Z80-WIRE-CAVEAT-ABSENT` row. This parcel does not edit
  lane state.
- **Re-open trigger:** a client reports that it treated a zero on a Z80-reachable range as a Z80 finding *despite* §6.
  That would be evidence that the prose is not read. Or the (D)/(F) deferral lands partly, which would make
  delivery a per-run fact rather than a permanent one.

## 6. A contradiction in the brief, reported rather than resolved

The dispatch brief asked, under case (A), for a gate proving the caveat **absent** on a watch over work RAM
(`$FF0000`). Both the contract and core say that work RAM **is** Z80-reachable. §6 `:1437` says *"cartridge ROM and
work RAM are all such addresses"*, and `Z80_REACHABLE_68K` includes `$E00000-$FFFFFF`. The adapter is pinned: the
bank window resolves work RAM. A faithful (A) would therefore have been *present* at `$FF0000`. The only way to make
it absent on the commonest watch is to misstate reachability. That is further evidence for (C). The brief's question
about a watch over only `$A04000-$A04003` resolves the same way. Z80 *writes* there are delivered, but its
YM-status *reads* are not, so an unfiltered or read watch there is covered by §6 and would draw A1's caveat.

## 7. Tags and what was not measured

- **TAG (live machine):** how many of aeon's real watch arms fall in `Z80_REACHABLE_68K`, and whether the Z80 is ever
  held for a whole capture. Both are reasoned from §11.52's consumer note and core's comment, not counted.
- Nothing was built, run or listened to. No land.sh run, since this is a docs-only parcel.

```detectors
ABSENCE: no caller of Watchpoints::caveats() in the Aether server source
  instrument: grep -rn 'caveats(' crates/oracle-aether/src | wc -l -> 0
  positive:   grep -rn 'caveats(' crates/oracle-player/src | wc -l -> 1
  negative:   grep -rn 'caveats(' crates/oracle-core/src/vdp.rs | wc -l -> 0
  scope:      every file under crates/oracle-aether/src at 012cfd9, recursively, all extensions
  contains:   the watch handlers live in crates/oracle-aether/src/engine.rs (fn watchpoint_hits at :8688), inside the grepped tree
HEURISTIC: the previous parcel's reading that the row "reduces to the conditional per-reply caveat"
  from:    the Z80-WATCH-TAP landing note
  assumes: that a conditional caveat is the §2.4-preferred shape for this limit after §11.52
  checked: 012cfd9:docs/OVERSEER.md:496 "reduces to the conditional per-reply caveat"
CANNOT-TEST: none — no impossibility is claimed; the one unmeasured quantity (aeon's watch ranges) is tagged for a live machine, not asserted untestable
```
