# `emulator/write_vram` — served, and the three absences it was served with

**Date** 2026-08-27 · **Branch** `parcel/write-vram` · **Base** `6568ca0`
**Contract revision read** `empyrean` `origin/main` = **`091ac59`** (`091ac592b0e30af98db261ef69fa7575961071a6`)

One of the 16 methods the suite contract describes and this server did not serve. Both halves of its
contract are final upstream, so this was pure conformance: no fork to adjudicate, and none invented.

---

## 1. What shipped

| Commit | What |
|---|---|
| `b292707` | `Vdp::poke_vram` in `oracle-core`; the `emulator/write_vram` handler + `METHODS` row in `oracle-aether`; `tests/write_vram.rs` (9 rows); the `SCHEMATIZED_NOT_ADVERTISED` pin updated |
| `a5d7e3b` | The watch-surface pin pair labelled with which one is sensitive (see §5, the one poison that came back green) |

Files: `crates/oracle-core/src/vdp.rs`, `crates/oracle-aether/src/engine.rs`,
`crates/oracle-aether/tests/write_vram.rs`, `crates/oracle-aether/tests/schema_conformance.rs`.

**No contract artifact moved.** The vendored `tests/contract/bus-protocol.schema.json` is already
byte-identical to `091ac59` (SHA-256 `df5d2a3e…e8b5`, verified against
`git -C empyrean show origin/main:contract/schema/bus-protocol.schema.json`), so no re-vendor was needed
and none was done.

**Currency did not move.** `git diff main -- crates/oracle-core/tests/` is empty. No golden fixture was
regenerated, and nothing pushed toward regenerating one. The new core-side unit tests live in
`src/vdp.rs`'s `#[cfg(test)]` module, not in `crates/oracle-core/tests/`.

---

## 2. The contract, as read at `091ac59`

The reading was done at a **committed revision**, never through `../empyrean/contract/...` — that path is
a peer's live working tree and would have carried uncommitted rules no other lane can see.

### The §6 row (line 1257)

```
| `emulator/write_vram` | `addr`, `bytes` | `addr`,`len` |
```

### The fragment (`methods["emulator/write_vram"]`)

`params`: `unevaluatedProperties: false`, `required: ["addr", "bytes"]`.

* `addr` — `$defs/hex`. *"First VRAM byte to write. D9 category 1. §6's row states no bound;
  `emulator/read`'s note gives the space size as `0xFFFF`. See audit D-16."*
* `bytes` — `"pattern": "^0x([0-9A-Fa-f]{2})+$"`. *"Byte payload — hex string, even digit count enforced
  by the pattern (D9 category 1). **The only payload spelling this row has.**"*

`result`: `allOf: [$defs/replyFields]`, `required: ["addr", "len"]`; `addr` = *"Echoed base."*,
`len` = `integer, minimum: 0` — *"Bytes written."* **No `caveat` is declared**, so emitting one fails §8
item 20's `unevaluatedProperties: false` closure (measured — see poison P5).

### The `$comment`, verbatim on the points relied on

> FIRST FRAGMENT, transcribed 2026-08-22. THREE ABSENCES ARE TRANSCRIBED RATHER THAN REPAIRED, and each
> is registered: (1) the row is NOT named in §6's run-control state rule, though `write_memory` and
> `write_cram` both are and §11.17's stated reason for naming `write_cram` — a game that composes its own
> state every frame overwrites a direct write inside the frame it lands in — is if anything stronger for
> VRAM (audit D-16); (2) no address bound is stated, though `emulator/read`'s note fixes VRAM at `0xFFFF`
> and every other write row in this catalog says what it refuses (audit D-16); (3) there is no
> `value`+`width` spelling, so unlike `write_memory` and `z80_write` this row takes a byte payload only —
> which is arguably right for a tile blit and is recorded as an asymmetry rather than a defect. A poke is
> a debugger access: on `write_memory`'s and `write_cram`'s standing rule it is never offered to the watch
> surface, and `watchpoint_hits.seen` does not move for it. `caveat` is declared ABSENT (the
> `write_memory` precedent).

### The cited neighbours

**§6's run-control state rule** (line ~933), quoted in full on the naming:

> `run_to`, `run_to_scanline`, `run_frames`, `step*`, `press`, `play_input`, `reload_rom`,
> `write_memory`, `write_cram` and `z80_write` *(named 2026-08-26, §11.22, audit D-11)* require a
> **paused** machine. Called while it is free-running they MUST fail with `-32005`
> (`data.reason = "machineRunning"`), never pause implicitly (§5).

Ten rows. `emulator/write_vram` is not among them. The same paragraph carries the rule that decided the
direction of the deviation:

> `write_memory` is named for `press`'s reason … Leaving them unnamed would let one server refuse and
> another accept, both conforming.

and, from §6's `write_memory` blockquote (line ~1052):

> Requires a **paused** machine (see the run-control state rule) — strict by design: **relaxing a refusal
> later is additive (D5); introducing one is not.**

**`emulator/read`'s space-size note** (line ~1014) — the source of the address bound:

> A base outside its space, or a range whose **end** runs past it, is `-32004` — **refused, never
> clipped**, because a clipped read reports bytes it never looked at. Space sizes: bus 24-bit, VRAM
> `$FFFF`, CRAM `$7F`, VSRAM `$4F`.

**§11.22** (2026-08-26), on how the neighbouring `z80_write` was made to refuse an overrun — the shape
this row's bound follows:

> `addr + len` above `$4000` is **refused whole before any byte lands** — the oracle lane's queue records
> that the legacy write *"silently corrupts on overrun"*, which is the harm.

**§11.17** supplies the second reason `write_cram` was gated, which the fragment says is stronger here
(quoted from §6's run-control paragraph):

> on a running machine a game that composes its palette every frame overwrites a direct CRAM write inside
> the frame it lands in, so the free-running call is not a weaker answer but a vanishing one.

---

## 3. How the three absences were served — as written, every one

| # | Absence | Served as | Why not "fixed" here |
|---|---|---|---|
| 1 | Row not named in §6's run-control state rule | **No pause gate.** A free-running call is served, and D11's stamp says `running: true` | Adding the gate would *introduce* a refusal the contract does not state — the direction §6 itself calls non-additive. Omitting it is the reversible half: the gate can be adopted the day the contract states it, without breaking a client |
| 2 | No stated address bound | **`-32004`, refused whole before any byte lands**, on the bound `emulator/read`'s note already states and `read_vram` (engine.rs) already enforces: `addr + len` must not exceed `$10000` | Some bound is physically unavoidable — 64 KiB of VRAM exists and no more. Adopting the read half's rather than inventing a second one is the smallest available choice and keeps the two twins refusing the same addresses |
| 3 | No `value`+`width` spelling | **`bytes` only.** `value` / `width` are *undeclared params*, refused by §2.5's closure with `-32602` and the offending keys named | The fragment records this as an asymmetry, not a defect. Adding the spelling would put a param on the wire that no fragment declares |

**A fourth observation, deliberately not treated as a fourth absence** (the fragment names three, and this
is not one of them): this row declares **no length ceiling**. `write_memory` bounds its payload by
`limits.maxWriteLen` and its result's `len` by `maximum: 4096`; this row's `len` is `minimum: 0` with no
maximum. Served as written — no `maxWriteLen` check. The row is not unbounded in practice: the address
bound caps one payload at 64 KiB. Recorded below as CR material at the lowest priority.

---

## 4. CR text, ready to send to empyrean

Three items, filed against audit **D-16**. Each is a request to state something the fragment already
argues for; none asks for a shape change.

> ### CR-WV-1 — name `emulator/write_vram` in §6's run-control state rule
>
> **Where** §6, the run-control state rule (line ~933); and the `$comment` on
> `methods["emulator/write_vram"]`, absence (1).
>
> **What** Add `write_vram` to the named list, beside `write_memory`, `write_cram` and `z80_write`.
>
> **Why** The fragment already makes the argument and declines to act on it. §11.17's stated reason for
> naming `write_cram` — *"on a running machine a game that composes its palette every frame overwrites a
> direct CRAM write inside the frame it lands in, so the free-running call is not a weaker answer but a
> vanishing one"* — transfers to VRAM with more force, not less: a game that rebuilds a plane or DMAs a
> tile block every frame overwrites a poked tile inside the frame it lands in, and the poke of a *sprite
> attribute entry* is overwritten by the guest's own SAT rewrite in the same vblank. Left unnamed, the row
> is exactly the case the rule's own paragraph warns about: *"Leaving them unnamed would let one server
> refuse and another accept, both conforming."*
>
> **What it costs the reference server** One line. `oracle`'s handler currently serves the free-running
> call, with a test (`write_vram.rs::it_is_not_subject_to_the_run_control_state_rule`) pinning that
> reading **to the contract**, not endorsing it — the day this CR lands that test goes red and forces
> `require_paused` into the same commit.
>
> **Note on direction** This is the introduction of a refusal, which §6 itself says is not additive (D5).
> The row has never been served by this server before today, and `oracle-old` does not serve it either, so
> the client population that could be broken is the one that starts today. Landing it soon is materially
> cheaper than landing it later.

> ### CR-WV-2 — state `emulator/write_vram`'s address bound and its refusal
>
> **Where** §6's *VRAM / CRAM / layers* row (line 1257) and/or a normative blockquote beside it; the
> `$comment`, absence (2).
>
> **What** State that `addr` is `0x0000`–`0xFFFF`, that `addr + len` above `0x10000` is **`-32004`,
> refused whole before any byte lands, never clipped, wrapped or truncated**, and that the space size is
> `emulator/read`'s (`VRAM $FFFF`).
>
> **Why** *"every other write row in this catalog says what it refuses"* — the fragment's own words.
> `write_memory` names its window and its code; `z80_write` names `addr` 0–`$3FFF` and had `addr + len`
> above `$4000` pinned as a whole-request refusal by §11.22 for a measured harm (*"silently corrupts on
> overrun"*). The VRAM harm is the same shape and worse-behaved: a server that masked `addr & 0xFFFF` —
> which is exactly what the core's legitimate guest write path does — would land the tail of an
> over-the-end payload at VRAM `$0000`, corrupting a byte the caller never named, with a success reply.
> Two conformant servers could ship opposite answers today.
>
> **What it costs a consumer** Nothing; it describes behaviour any usable server must already have.
> `oracle` enforces this bound now, and pins the no-wrap case directly.

> ### CR-WV-3 (lowest priority) — say whether this row has a length ceiling
>
> **Where** the §6 row; the fragment's `result.len`.
>
> **What** Either state that `limits.maxWriteLen` applies (and bound `result.len` accordingly), or state
> that it deliberately does not and that the address bound is the only cap.
>
> **Why** `write_memory` bounds its payload by `limits.maxWriteLen` and its `result.len` by
> `maximum: 4096`; this row's `result.len` is `minimum: 0` with no maximum, and the row states no ceiling.
> A byte-payload blit plausibly wants the whole space in one call, so the asymmetry may well be
> intentional — but it is currently indistinguishable from an omission, and a client cannot tell whether a
> 64 KiB payload is a supported call or a refusal waiting to happen.
>
> **What it costs a consumer** If the answer is "`maxWriteLen` applies", a client that today sends a
> whole-space blit starts getting `-32602` and must chunk. `oracle` serves the no-ceiling reading, which
> is the one the fragment's `minimum: 0` / no-maximum spelling states.

A fourth item is **not** raised: absence (3), the missing `value`+`width` spelling. The fragment records
it as a deliberate asymmetry and gives the reason (a tile blit is a byte payload). Concur — no CR.

---

## 5. The `vram_mut` verdict: **the claim was TRUE**

The prior survey's claim — that `vram_mut()` is a pub-ised test hatch bypassing the SAT-cache
write-through — is **correct**, verified firsthand against `crates/oracle-core/src/vdp.rs`.

`vdp.rs:394` (its own doc comment already says what it is):

```rust
/// Mutable access to VRAM (used by tests to perturb state; the data-port write path lands in a later
/// slice). Kept crate-internal-friendly but public for the `System::vram_mut` pass-through.
pub fn vram_mut(&mut self) -> &mut [u8] {
    &mut self.vram
}
```

It returns the bare array and runs nothing else. Every **guest** VRAM byte instead routes through
`Vdp::write_vram_byte` (`vdp.rs:781`), which after storing the byte mirrors the *cached half* of a sprite
attribute entry — bytes 0–3 of the entry, Y + size/link — into `sat_cache`, the copy `sprites_decoded`
(`render.rs:1067`) reads `y`, `widthCells`, `heightCells` and `link` from. A poke through `vram_mut` into
the SAT window would leave the cache describing the previous sprite: `emulator/sprites` would report the
**old** `y` and `cacheDivergence: true` for a table nobody had left stale, and the renderer would draw a
picture the VRAM does not describe.

So the bus does not use it. `Vdp::poke_vram` was added: same store, same SAT write-through, and
**deliberately no `capture`** — a poke is a debugger access and a watch hit's `pc` names an instruction a
poke does not have. It asserts rather than masks an out-of-range address, so a server bug surfaces as a
panic instead of a silent wrap.

The SAT arithmetic is **duplicated** from `write_vram_byte` rather than factored into a shared helper, on
`poke_cram`'s explicit precedent: `write_vram_byte` is on the **currency path** (every guest write and
every DMA byte routes through it), and `vram_poke_matches_the_port_path` is the test that stops the two
copies from drifting. See §7 for the alternative and its cost.

---

## 6. Verification — every gate, with real numbers

### Gates

| Gate | Result |
|---|---|
| `cargo fmt --all` | clean; `cargo fmt --all -- --check` → `FMT_CLEAN` |
| `cargo clippy --workspace --all-targets` | **0 warnings, 0 errors** |
| `cargo test --workspace --no-fail-fast` (baseline, `main`) | **LEGS 56 · PASSED 1900 · FAILED 0 · IGNORED 6 · exit 0** |
| `cargo test --workspace --no-fail-fast` (branch) | **LEGS 57 · PASSED 1912 · FAILED 0 · IGNORED 6 · exit 0** |
| `git diff main -- crates/oracle-core/tests/` | **empty** |

**The recorded hint was stale and checking mattered**: a recent doc records `PASSED=1880`; re-derived on
`main` in this worktree it is **1900**. The baseline in the table above is the re-derived one.

### The delta, test by test

**+1 LEG.** `oracle-aether`'s `tests/write_vram.rs` is a new integration-test binary, so the workspace
gains one leg. It is the **only** leg that differs — the two runs' `Running …` lines were diffed as sets,
and no leg vanished.

**+12 PASSED**, all named — the delta is exactly the twelve rows below and nothing else:

*`crates/oracle-aether/tests/write_vram.rs` — 9 new rows (the new LEG):*

1. `bytes_land_in_vram_and_read_back_through_both_read_paths`
2. `the_key_set_is_exact_and_carries_no_caveat`
3. `the_address_bound_is_refused_whole_and_never_wrapped`
4. `the_bound_is_the_cores_own_vram_size`
5. `bytes_is_the_only_payload_spelling_and_a_refusal_writes_nothing`
6. `an_undeclared_payload_key_is_named`
7. `a_poke_into_the_sprite_table_maintains_the_sat_cache`
8. `a_poke_is_never_offered_to_the_watch_surface`
9. `it_is_not_subject_to_the_run_control_state_rule`

*`crates/oracle-core/src/vdp.rs` `#[cfg(test)]` — 3 new rows (existing LEG, `oracle-core --lib`):*

10. `vdp::tests::vram_poke_matches_the_port_path`
11. `vdp::tests::a_vram_poke_is_never_offered_to_the_watch_surface`
12. `vdp::tests::a_vram_poke_past_the_end_panics_rather_than_wrapping`

`1900 + 12 = 1912`, and the measured aggregate is 1912 — the delta reconciles exactly, with no unexplained
movement. Several **existing** rows now cover one more method each (`params_closure`'s two per-method
sweeps and `schema_conformance`'s coverage split each iterate one more advertised method, and
`mcp_tool_sweep` one more fragment), but those are loops inside a single test row and move no count.

*A note on how this figure was reached, because the first draft of this table got it wrong:* an earlier
version of this document carried `1914` and an invented explanation for the extra two. That number was
**predicted, not measured**. The run says 1912. Predicting a total and then reasoning backwards to justify
it is the exact failure the "report the aggregate" rule exists to prevent, and it is recorded here rather
than quietly corrected.

### Red-first: every added assertion, and the poison it failed against

Each poison was applied to source, the test run, the failure recorded, and the source reverted
(`git checkout --`). Two poisons initially came back green and both were chased down.

| # | Poison | Test(s) proven red | Outcome |
|---|---|---|---|
| P1 | Delete the SAT-cache write-through from `poke_vram` (i.e. make it behave exactly like `vram_mut`) | `vdp::tests::vram_poke_matches_the_port_path`; `write_vram.rs::a_poke_into_the_sprite_table_maintains_the_sat_cache` | **RED** both — *"the two copies of the window arithmetic have drifted"*; *"`y` is read from the cache, so a `vram_mut` write would still report 128 here"* |
| P2 | Add a `capture(…)` call inside `poke_vram` | `vdp::tests::a_vram_poke_is_never_offered_to_the_watch_surface` | **RED** — and `write_vram.rs`'s namesake stayed **GREEN**; see §5 note below |
| P3 | Mask `addr & (VRAM_SIZE-1)` in the core **and** delete the bus's end-bound refusal (wrap instead of refuse) | `vdp::tests::a_vram_poke_past_the_end_panics_rather_than_wrapping`; `write_vram.rs::the_address_bound_is_refused_whole_and_never_wrapped`; `write_vram.rs::the_bound_is_the_cores_own_vram_size` | **RED** all three — *"`emulator/write_vram` unexpectedly succeeded"* |
| P4 | Open `MethodSpec.params` to `["addr","bytes","value","width"]` | `write_vram.rs::bytes_is_the_only_payload_spelling_and_a_refusal_writes_nothing`; `write_vram.rs::an_undeclared_payload_key_is_named`; `params_closure::every_advertised_method_declares_exactly_its_fragments_params` | **RED** all three — *"`emulator/write_vram`: the accepted key set and its fragment disagree"* |
| P5 | Emit a `caveat` the fragment declares absent | `write_vram.rs::the_key_set_is_exact_and_carries_no_caveat` | **RED** — and doubly: the vendored-schema wire validator fired first, *"`methods.emulator/write_vram.result: $: Unevaluated properties are not allowed ('caveat' was unexpected)"* |
| P6 | Invent the pause gate — add `require_paused("emulator/write_vram")` | `write_vram.rs::it_is_not_subject_to_the_run_control_state_rule` | **RED** |
| P7 | Reverse the payload byte order (`data.iter().rev()`) | `write_vram.rs::bytes_land_in_vram_and_read_back_through_both_read_paths` | **RED** |
| P8 | Leave `"emulator/write_vram"` in `SCHEMATIZED_NOT_ADVERTISED` | `schema_conformance::the_schema_covers_every_method_we_advertise_and_the_uncovered_list_is_pinned_empty` | **RED** — *"the set of schematized-but-unadvertised methods changed"*; this is the pin that forced the edit in the shipping commit, exactly as its own comment says it should |

**Two false greens, both chased rather than accepted.**

*P5's first run came back green because the poison landed on the wrong function.* The anchor
`Ok(json!({ "addr": hex::addr(addr), "len": data.len() }))` is `write_memory`'s return as well as
`write_vram`'s, and the surrounding lines matched `write_memory`'s. Re-applied with an unambiguous anchor,
the test is red. **The lesson is about the poison, not about the test** — a green from a misplaced poison
is indistinguishable from a weak assertion until you look.

*P2's aether-level green is real, and is a property of the harness rather than of the assertion.*
`System::run` arms the VDP capture buffer for the duration of a run and disarms it on return
(`system.rs:1011` / `system.rs:1105`); a poke is dispatched *between* runs, so it meets a disarmed buffer
whatever it does, and nothing reachable from the bus surface can arm it around a poke. The end-to-end test
therefore pins the observable contract property (a live watch's `seen` and `matched` do not move) but
**cannot** catch a `capture` call inside `poke_vram`. The core-level test can, and does. Both are kept,
and commit `a5d7e3b` writes that distinction into both doc comments — a green standing for something it
does not prove is precisely what this pass exists to catch.

**A pre-existing finding, measured while establishing that** — reported, not fixed, because it is not this
parcel's row: `crates/oracle-aether/tests/cram.rs::a_poke_is_never_offered_to_the_watch_surface` is
insensitive in exactly the same way. Poisoning `Vdp::poke_cram` with a `capture(VdpTarget::Cram, …)` call
leaves it **green**, and `poke_cram`'s doc comment calls it *"the direct pin"*. The fix is one unit test
beside `poke_cram`, on the model of the one added here.

### Conformance rows really ran

`vendor` is symlinked to `/home/volence/sonic_hacks/oracle/vendor` and `vendor/TestRoms` holds **17**
entries, so `conformance_roms.rs` did not silently SKIP.

---

## 7. The better-approach pass — where we could beat the fragment, and what it would cost

Run as required, and recorded as recommendation only. **Every one of these was declined and the fragment
served instead.**

1. **The pause gate (absence 1) is the one place the fragment is arguably wrong, and shipping it anyway
   was still right.** A free-running `write_vram` is a call whose effect a game erases inside the frame it
   lands in; §11.17's own argument for `write_cram` applies with more force. But "better" here means
   *refusing more*, and a refusal introduced ahead of the contract is not discoverable by a client and not
   reversible without breaking one. **Cost of deviating:** a client that legitimately pokes VRAM
   free-running against this server — a plausible use for a tile-blit debug loop — gets `-32005` from us
   and success from any other conformant server, with no way to tell which is right. Filed as CR-WV-1
   instead; the pin goes red when it lands.

2. **`result.len` could carry the *pre-write* bytes it replaced, or a `changed` count.** A poke that
   writes bytes already equal to the payload is indistinguishable from one that changed the picture, and
   `read_cram`'s and `read`'s echo precedents show this catalog does like a self-describing reply.
   **Cost of deviating:** an undeclared result key fails §8 item 20's closure at the schema — measured in
   poison P5, which the vendored validator caught before the assertion did. It cannot be done
   unilaterally at all; it would have to be a CR first. Not raised: the value is speculative and the row
   is `addr`+`len` by design.

3. **A `words` spelling beside `bytes`, for the data port's odd-address byte-swap.** VRAM's real write
   unit through the port is a word, and an odd-address word write swaps its two bytes (recon R3). A
   client blitting a tile map thinks in words. **Cost of deviating:** a param no fragment declares →
   `-32602` from §2.5's closure on any other server, and a second byte-order rule for clients to
   remember. §11.22 ruled against exactly this shape of symmetry for `z80_write` (*"a `width` companion
   would add a rule clients must remember for a case `bytes` already covers"*). Declined on that
   precedent; not raised as a CR.

4. **Sharing the SAT write-through between `write_vram_byte` and `poke_vram` instead of duplicating it.**
   Single source of truth is the better engineering answer in the abstract, and the duplication is real.
   **Cost:** it is an edit to a function on the **currency path** — every guest VRAM write and every DMA
   byte routes through `write_vram_byte`, and every frozen golden depends on it. `poke_cram` faced the
   identical choice and chose duplication plus a cross-test; this follows that precedent, with
   `vram_poke_matches_the_port_path` as the drift guard, proven red against P1. **Recommendation:** if a
   third VRAM writer ever appears, the extraction earns its currency re-verification; two do not.

5. **Length ceiling (the fourth observation).** Serving `maxWriteLen` here would be tidier and matches
   `write_memory`. **Cost of deviating:** a whole-space blit that every other conformant server accepts
   would be refused by us alone. Filed as CR-WV-3 at lowest priority, asking upstream to *state* an
   answer either way rather than proposing one.

---

## 8. What is left open

* **CR-WV-1 / CR-WV-2 / CR-WV-3** are written above and not yet sent. Sending them is an empyrean-side
  action, outside this worktree.
* **The `cram.rs` watch-surface pin is insensitive** (§6). Not fixed here — different row, and fixing it
  means adding a unit test beside `poke_cram`, which is a one-line-scoped follow-up someone should take
  deliberately rather than as a rider on this parcel.
* **`tests/contract/PROVENANCE.md` says "62 fragments"; the vendored file parses to 63.** The bytes are
  correct and byte-identical to `091ac59`; only the prose count in the provenance table is stale. Not
  touched — it is a re-vendor artifact and editing it outside a re-vendor commit would make the record
  worse, not better.
* **⟨RUNTIME⟩ — nothing.** Everything here was driven in-process through `Engine::dispatch` over the test
  harness's real socket client. No emulator MCP tool was touched, no server was spawned, and no assertion
  in this parcel needs a running machine that the harness does not already provide. There is nothing
  queued for foreground follow-up.
