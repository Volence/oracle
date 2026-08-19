# Aurora's first demand-side statement: the CRAM surface, and a footgun (2026-08-19)

Recorded for the same reason the Aeon gap list, the scanline-readback demand and the
profiler demand were: a demand-side statement of what this bus is missing, **evidenced
rather than asserted**. Aurora is the suite's editor, and this is its first appearance as
an Aether client — the first non-MCP one.

Aurora repo state at transcription: `aurora` @ `677ad66`.
Relay source: the Aurora session, 2026-08-19 (three messages — the probe results and the
prioritized request list; then a same-day correction which **de-prioritized its own top
item**, folded in below at §1 and §5 in the house's supersession style with the original
ask left visible under it; then a second relay, §6, carrying a listing-format change
Aurora had itself been told by the Aeon session).
This server at transcription: `oracle-next` @ `6e399fd`, 32 advertised methods.
Contract at transcription: `empyrean` @ `d72513c`.

---

## 0. Why this is a demand and not a feature idea

Aurora did not ask from a design document. It **connected to the live server**, ran the
two-step handshake, read the advertised method list, and called things until they failed.
Every item below carries a probe result rather than a wish. That is the same evidentiary
standard the three Aeon demands met, and it is why this is filed as a demand.

It also matters *who* is asking. Until today every client on this bus was the MCP —
which is, by D10's own framing, the surface the protocol was extracted **from**. Aurora is
the first client that learned the bus from `initialize` and the contract, with no shared
history with the server. What it tripped over is therefore evidence about the *contract's*
legibility, not about one client's habits.

---

## 1. The six items as filed — and the requester's own correction

### Item 1 — serve `emulator/write_cram` ~~(BLOCKING their phase)~~

**As filed:** the only thing gating their phase. `emulator/write_cram` answers `-32601`
today. It is schematized at `contract/protocol.md:1050` and carried in our vendored copy
of the schema at `crates/oracle-aether/tests/contract/bus-protocol.schema.json:723` —
**both anchors verified at the current revision**. Their designated demo was the "one
product" demo of the suite plan: drag a palette slider in the editor, watch the running
game recolour on the next frame.

> **★ SUPERSEDED THE SAME DAY, BY ITS OWN REQUESTER.** Aurora withdrew the *blocking*
> claim after checking what a CRAM write would actually survive on a running machine. Two
> anchors, both verified read-only in their own trees for this document:
>
> - `aeon/engine/effects/palette.emp:31` — *"One owner, one deterministic order, composed
>   once per frame into Palette_Buffer"* — with `aeon/engine/ram.emp:973` recording a
>   *"Per-line copy of Palette_Buffer taken at each line's frame-top DMA enqueue"*. A
>   direct CRAM write on a **running** Aeon machine is overwritten within a frame, and so
>   is the composed buffer: the live hook has to target the **source** of the pipeline,
>   which is an Aeon-side design question (their design #8's DEBUG-override block), not a
>   bus question.
> - `s1disasm/sonic.asm:1034` — `HBlank:`, the classic Sonic 1 horizontal interrupt,
>   *"exclusively used for the LZ water palette effect"* (`:1030`), which blasts
>   `v_palette_water` straight to CRAM through the data port, with the dry equivalent in
>   VBlank. A CRAM write there survives well under a frame.
>
> **Their conclusion, in substance:** the live-palette demo is a `write_memory`-to-RAM-source
> story, and `write_memory` already serves it. **Nothing of Aurora's is blocked on us.**
>
> `write_cram`/`read_cram` therefore stay in our plan as **contract debt at our own
> priority**, not as a client unblock. That rationale — retiring the last survivor of the
> CR-18-era schematized-but-not-served count mismatch — stands on its own and is argued in
> the recon doc beside this one.

The original ask is left above rather than rewritten because the *shape* they asked for is
still the shape we would build, and because a demand register that quietly edits its own
entries stops being evidence. What changed is the priority and the reason, not the design.

### Item 2 — serve `emulator/read_cram`

`line?` (0–3) → `palette[]`. Wanted for palette read-back on connect and to verify a write
landed. Their controller grep-confirmed what our own dispatch table shows: **neither
method has a handler in `engine.rs`**.

One correction to their evidence, in their favour and against ours: they described both
methods as "schematized". `write_cram` is. **`read_cram` is not** — it has a §6 row
(`protocol.md:1049`) and **no schema fragment at all**. The archaeology is in the recon
doc; the practical consequence is that item 2 costs a fragment and item 1 does not.

### Item 3 — the `write_memory` unknown-params footgun

The sharpest item in the list, and the one that survives their own correction untouched.
Verified by their probe against the live server:

- `{symbol: "Player_1", offset: 2, value: …, width: 2}` → **succeeds**, writes to
  `Player_1` + 0.
- `{symbol: "Player_1", disp: 2, value: …, width: 2}` → **succeeds**, writes to
  `Player_1` + 0.
- `{symbol: "Player_1+2", …}` → **correctly refused** (`-32011`, no such symbol).

Unknown top-level keys are dropped on the floor and the client is told OK. Their
play-from-cursor warp writes `Player_1`+`$02` and +`$06`; a client guessing a parameter
name therefore corrupts a *different* player field and receives a success reply naming an
address it did not ask to write.

We reproduced the mechanism in source rather than taking the probe on trust:
`Engine::resolve_target` (`crates/oracle-aether/src/engine.rs:986-1007`) reads `symbol`,
else `addr`, and nothing else; `write_memory` (`:1284-1355`) reads only
`bytes`/`value`/`width` beyond that. There is no key-set check anywhere on the params
path. The probe is exactly right about both the behaviour and the cause.

Their sharpening, quoted because it reframes the fix and we would otherwise have built
only half of it:

> a `disp` param and reject-unknowns answer **different halves** — the footgun half is
> what bit them.

An explicit displacement parameter serves the *ergonomic* half (their warp wants
`Player_1`+2 and has to compute the hex address itself today). Rejecting unknown params
serves the *safety* half (a guessed name must not be silently ignored). Adding `disp`
alone would leave the next client's `offset:` guess just as silently wrong.

### Item 4 — whole-line batch `write_cram` ~~(offered, deferrable)~~

**Withdrawn by the requester.** Filed as a 16-entries-per-call convenience with an
explicit "we're happy with singles"; retracted the same day with a reason, once item 1's
demo moved: **the drag loop it was sized for is not a CRAM loop.** Recorded here as
withdrawn-with-reason, not as deferred-by-us — the distinction matters, because a
deferral is a debt we owe and a withdrawal is not.

### Item 5 — papercut: the `ORACLE_SOCKET` length error

An over-long `$ORACLE_SOCKET` fails with a raw `path must be shorter than SUN_LEN`. That
string is Rust std's, surfaced unmodified from `UnixListener::bind`
(`crates/oracle-aether/src/server.rs:344`) — we neither wrote it nor name the actual
limit. Ask: say what the limit is.

### Item 6 — QUESTION, not a change: what does `load_symbols` bind against?

They intend to feed it an AS listing from a **stock Sonic 1 disassembly** build, and want
the expected failure modes before they try.

> **★ This is now their ONLY decision-gating item.** With item 1 de-prioritized by them
> and item 4 withdrawn, the answer to item 6 is what decides whether stock-S1 AS listings
> are viable for classic-side Build & Run at all. It is answered from source in
> `docs/2026-08-19-cram-serve-recon.md` §6, written to be relayed to the Aurora session
> verbatim. **The short version is that the answer is "no, not today, and the blocker is
> not the check you asked about"** — see there.

---

## 2. Their two self-corrections, recorded

A demand register that only records what a client got wrong about *us* is a biased
instrument. Both of these were volunteered.

1. **The missed `initialized` step.** Their first probe sent `initialize` and then began
   calling methods, and saw a healthy connection that never delivered an event. They found
   the cause themselves: `initialized` is a separate notification and the server does not
   push to a connection that has not sent it. That is D6's rule working exactly as
   specified (`protocol.md:84-89`), and our state machine implements it at exactly one
   transition — `Session::on_message`'s `"initialized"` arm sets `ready` and returns
   `Action::Subscribe` (`crates/oracle-aether/src/session.rs:79-93`). Not a defect. It did
   cost them a debugging session, which is what item 7 below is about.
2. **The measurement basis.** Their first-pass numbers were taken against a headless
   server, not against the windowed player, and they flagged it before we could. Any
   latency figure they quote is therefore a floor, not a player-loop measurement.

---

## 3. What they verified working, and want kept

Recorded because a demand list read alone implies a surface that is failing, and this one
is not.

- **The two-step handshake.** Once corrected, `initialize` → `initialized` → dispatch
  behaved exactly as §2.1 describes, including the event subscription.
- **The 32 advertised methods as their feature-detection basis.** They enumerate
  `capabilities.methods` from `initialize` and branch on it — never on a version integer,
  which is D5's rule (`protocol.md:78-82`) being honoured by a client that had no
  particular reason to know it. **Their explicit ask: keep the advertised list
  authoritative.** They are relying on D4's guarantee that a method absent from that list
  is genuinely not there, and on the converse. Our own conformance harness already treats
  the advertised list as the authority for coverage and prints the *schematized but not
  advertised* set beside it as a smell rather than a promise
  (`crates/oracle-aether/tests/schema_conformance.rs:162-187`) — which is the same rule
  seen from the server side. Serving `write_cram` and `read_cram` shrinks that smell set
  to empty; **it must not do so by loosening what the list means.**
  *Their client-core note, added with the second relay:* feature detection off the
  advertised list is **live on their side now**, not planned — `write_cram` will light up
  in their client automatically the day we advertise it, with **no coordination needed**.
  The old C++ Oracle's MCP behaves the same way for the same reason
  (`oracle/linux-port/mcp/oracle_mcp.py:879-904` filters its tool rows against the
  server's advertised `methods`), so advertisement is now the single switch on two
  independent clients. That is D4 paying off, and it is also a warning: advertising a
  method is shipping it.
- **D7 server-side symbol resolution**, named as a property they want kept rather than as
  a request. They pass `symbol` and let the server resolve, per D7 (`protocol.md:91`),
  instead of caching addresses in the editor. Item 3 is a consequence of taking D7
  seriously: they wanted `Player_1`+2 to be a *symbol-relative* request and reached for
  the nearest-looking parameter name.

---

## 4. Item 7 — the optional papercut they offered

Not a request; offered as a courtesy to the next client author.

A client that sends `clientCapabilities: {events: true}` in `initialize` and then never
sends `initialized` sees a completely healthy connection that never receives an event. One
line to stderr when a connection dispatches its first ordinary method while
`wants_events && !ready` would have saved them the session in §2.1. Feasibility is
assessed in the recon doc (the state machine is pure by design and does no I/O, so the
warning belongs in the connection loop, not in `Session`).

---

## 5. What this bus owes Aurora after the correction

**Nothing, on the blocking axis.** That is worth stating plainly, because the first
version of this document opened with a client blocked on us and the corrected version does
not. The `write_memory` footgun (item 3) is the one item that is a live defect of ours
rather than a missing feature, and it is the one item their correction did not touch.

**One new item, recorded but NOT requested.** Their correction carries a consequence they
raised themselves and explicitly did not ask us to solve: if the live palette is
`write_memory` to a RAM source, and `write_memory` is `require_paused`, then a smooth 30 Hz
slider drag is **30 pause/write/resume cycles per second — 60 `emulator/stopped` +
`emulator/resumed` events per second broadcast to every subscriber on the bus.** Their v1
plan is to throttle hard (10 Hz, or coalesce-on-idle), measure, and come back only with
numbers. They named two possible future shapes and asked for neither: relaxing the pause
gate for small bounded writes, or an apply-at-next-frame-boundary write. Both are
registered as an unscheduled follow-up in the recon doc with the measurement that would
revive them.

They offered to point their probe at a branch before it merges. Taking that offer is in
the recon's slice plan.

---

## 6. Second relay — the `.lst` format changed under all of us today

Aurora's third message is not a request at all. It is a **warning they passed on**, having
received it from the Aeon session: as of `sigil` `0df77f83` (2026-08-19), every `.lst`
listing carries a **third section** after the symbol table — `EQU <name> = $XXXXXXXX` rows
and an `N equates` trailer, 671 of them in `s4.debug.lst`. Sigil's position, relayed: these
are **values, not addresses**, and *must never resolve as code/RAM addresses*; their own
debugger-map test now pins that.

Recorded here with **relayed-from-Aeon provenance**, and relayed claims are not evidence in
this register — so it was verified from the primary source for the recon doc, and it holds
in every particular. `sigil` `0c56ba10` *"feat(listing): equates reach the .lst — an Equate
Table section"* (16:21), merged as `0df77f83` (16:22), both ancestors of sigil HEAD; the
Aeon listings were rebuilt minutes later (`aeon/s4.lst` 16:24, `aeon/s4.debug.lst` 16:23)
and carry **670** and **671** `EQU` rows respectively. The pre-change snapshot at
`.aeon-nightly/s4.debug.lst` (04:40 today) carries zero, which dates the change to within
the hour.

Aurora ranked three possible consequences by how quietly each would fail, which is a good
instinct and is why the warning was worth passing on. The findings — including which of the
three actually happens (none of them, by luck rather than design), what it does break
instead, and the one sentence of our own documentation it falsified four hours ago — are in
`docs/2026-08-19-cram-serve-recon.md` §6.3, inside the relay-ready Q6 answer.

Their design point travels with it and is recorded as theirs: equates are a genuinely
different **namespace**, and a same-named equate/label collision should be resolved
deliberately rather than by insertion order. If we ever ingest equates as a feature, that
is a decision for a ruling, not a parser accident.
