# The Aeon switchover gap list (2026-08-17)

Relayed by the owner from the Aeon side: the bus capabilities Aeon development uses regularly, ranked
by what unblocks their work soonest. Recorded here because it is the first **demand-side** statement
of what `oracle-next`'s bus is missing — every prior ranking of capabilities was written from this
side of the socket.

**Provenance:** tiers 1-3 and the two caveats are the Aeon side's, relayed verbatim in substance.
Everything under "Verified" and "Assessment" is this repo's, checked against the tree on 2026-08-17.
Where the two disagree, this document says so rather than smoothing it over.

## Verified — the gap list is accurate

Our Aether server serves **26 methods + 2 events** (`checkpoint`, `checkpoint_drop`,
`checkpoint_list`, `hold`, `load_symbols`, `lookup_symbol`, `pause`, `pixel_attribution`,
`play_input`, `press`, `read`, `read_memory`, `read_vram`, `registers`, `release_all`, `reload_rom`,
`restore`, `resume`, `run_frames`, `run_to`, `screenshot`, `sprites`, `state_hash`, `status`,
`watchpoint_add`, `watchpoint_clear`, `watchpoint_hits`, `watchpoint_list`; events `resumed`,
`stopped`).

Every method the list names as missing **is** missing — confirmed by grep, not by memory:
`write_memory`, `reset`, `memory_hash`, `step`, `step_over`, `step_out`, `run_to_scanline`,
`z80_read`, `z80_registers`, `write_cram`.

Their read of what already exists is also correct: watchpoints, `run_to`, and full
checkpoint/restore are shipped.

**Their Tier-2 item 7 question is answered YES.** `emulator/read` with `space:"cram"` reads
`sys.vdp().cram()` — live committed state, no frame latch (`crates/oracle-aether/src/engine.rs`, the
`read` handler). Oracle's frame-latched CRAM read was the vacuous instrument they suspected; ours is
not, and `space` already covers `bus`/`vram`/`cram`/`vsram`. The missing half is **writing**.

## The list

### Tier 1 — unblocks their pixel gate (their current queue item)

1. **`emulator/write_memory`** — the poke primitive. All three of their committed scenes poke;
   `mega_bus().write8` already exists in-tree and is used by `replay_runner`. **Their stated blocker.**
2. **`emulator/reset`** — scene preambles need a cold start. `System::reset()` exists, unexposed.
   Independently, this repo's own books already called `reset` "the conspicuous absence from a
   25-method control surface", so the two sides converged on it separately.
3. **`emulator/memory_hash`** — region hashing for gates. The FNV code exists in `state_hash`.

With those three, their `ab_runner` re-points at `oracle-next` and their screenshot gate closes
"without the C++ surgery".

> **Tier 1 SHIPPED 2026-08-18.** All three methods are live on the bus, contract-first (§11.13 in
> `empyrean`, schema re-vendored, handlers after). The MCP coverage sweep runs **31-for-31** through
> the real `call_tool` path against `aeon/s4.debug.bin`. Handoff:
> `docs/2026-08-18-tier1-bus-methods.md`. **The `ab_runner` re-point is now unblocked** — and running
> it is the finish line nobody has crossed yet; the sweep proves our server answers, not that their
> gate closes.
>
> Tier 2 item 4 (instruction stepping) is **still unruled** and the collision below still stands:
> do not build it without a ruling.

### Tier 2 — their daily debugging loop

4. **Instruction stepping** — `step`, `step_over`, `step_out`. **See the collision below.**
5. **`run_to_scanline`** — their effects/parallax work stops mid-frame. Pairs with a finding of
   theirs: attribution *at a scanline*, since `pixel_attribution` is frame-state-blind to raster
   bands by design. (This repo already has `F-TRACE-VDPWRITE-MCLK` / per-scanline capture as
   prerequisites in its own backlog — the two asks are the same shape.)
6. **Z80 surface** — `z80_read`, `z80_registers`. "Sound work is untouchable without it."
7. **CRAM/VSRAM write** — plus the live-read confirmation, answered above.

### Tier 3 — instruments, later

8. A frames-dir equivalent on the bus (per-checkpoint rastered frames) for `replay_framediff`.
9. Aeon-aware conveniences (`object_list`, `object_slot`, `player_state`), `log_tail`, layer
   toggles, profiler. **Note:** this repo previously flagged the first three as *game-specific* —
   Sonic object RAM at fixed offsets — and asked whether they belong on a game-agnostic bus at all.
   That question is still open and should be answered before anyone ports them by reflex.

## Assessment — two collisions with standing rulings

**1. `write_memory` was never a build task; it was an unruled question. RULED TODAY.**

This repo's keep-dead list carries "a register-write op", with the note that *its absence forced a
better answer twice*; and separately records that whether that entry covers **memory** writes "was
never ruled". So their critical-path blocker was a decision nobody had made.

**Owner ruling, 2026-08-17: ADOPT, scoped.** The keep-dead entry covers **register** writes only,
not memory writes. `emulator/write_memory` is to be built **contract-first** — a §6 contract row and
schema fragment before the handler — which is the measured discipline: of 21 methods audited, the
only ones with zero undocumented result keys were those specified before they were implemented.

The demand is evidenced rather than asserted (three committed scenes already poke through
`mega_bus().write8`), which is the condition the keep-dead entry's "forced a better answer" caution
was really about.

**2. Tier 2 item 4 — instruction stepping — collides head-on with an explicit keep-dead entry.**

"Interactive step/break/call-stacks/rewind" sits on the same list, under *do not let momentum
re-fund these*. **Not ruled.** There is a real distinction available — a scripted bus client
stepping is not the interactive debugger UI that entry was written against — but that is an argument
to be made and adjudicated, not assumed. **Do not build items 4 without a ruling.**

## An item the Aeon side cannot see: a contract row we now owe

The player's sprite-outline lens was changed on 2026-08-17 to outline the **sprite link walk**
(`render_line_report().sprites`) rather than all 80 SAT entries, because outlining the table drew
boxes around stale slots the hardware never displays.

An earlier ruling deferred the walk **with a trigger**: "it gets a contract row when something
renders it." That trigger has now fired. Under item 19 / D15 parity, the walk needs a §6 row — this
is work this repo owes regardless of the switchover, and it belongs in the same queue.

## Their two caveats — both accepted

- **Absolute band-edge claims keep `oracle` as the reference instrument** until this core's
  instruction-granularity slop closes: A/B gates cancel the slop, absolute measurements do not.
  Consistent with this repo's own cycle-granularity record; no argument.
- **The S/H exhibit.** ⚠ **Still needs disambiguation — but my first guess now looks wrong, and it is
  recorded here so nobody inherits it as fact.**

  I initially assumed they meant the shadow/highlight divergence from the *lens* work: the CRAM strip
  reads `Vdp::cram_decoded()` (the Normal ramp) while the renderer's S/H-aware `cram_rgb_state` is
  **private**, so the two agree everywhere except S/H regions — pinned by the core's own
  `cram_rgb_matches_cram_decoded`, and open only on whether to export the private conversion.

  **Evidence now points elsewhere.** A concurrent session landed `2c210e8` (HINT bookkeeping moves to
  the H anchor) and `2275b82` (`sh_probe` — a HInt raster **and S/H** diagnostic), out of an
  owner-reported OJZ water-line bug where the line rendered as binary whole-screen shadow. That work
  also flagged a **stale Aeon-side comment** (`ojz_effects.emp` — "S/H has nothing to dim", false
  because plane B is all low-priority). An S/H exhibit "from your session" much more plausibly means
  *that* session than the lens one.

  **Do not file either as a renderer bug until the Aeon side names the exhibit.** The two candidates
  have nothing to do with each other: one is an introspection instrument deliberately showing the
  Normal ramp, the other is raster-timing behaviour that has just been changed.

## Three-surface parity (owner rule, 2026-08-17)

**When we build something for MCP, it must work for plain Aether too — and where possible get a
surface in the player's GUI.** Applies in every direction; the surface that prompted the work is not
the only one that has to end up with it.

This repo has drifted both ways and measured it:

- **Bus without client.** The MCP coverage gap was **12 methods, 3 of them one day old** —
  `sprites`, `play_input` and `read` each shipped with a contract row, a schema fragment and a
  mutation-verified suite, and no client could call any of them the next morning. Item 19 makes a
  capability carry a *row*; nothing made it carry a *client*.
- **GUI without bus.** The player has had soft reset on `Tab`/`F1` for ages, while
  `emulator/reset` — Tier 1 item 2 of this very list — does not exist on the bus. The Aeon side is
  blocked on a capability that has been sitting in the player the whole time.

So each Tier 1 method is planned as four steps, not two: **§6 contract row → Aether handler → MCP
tool row → player surface (if one makes sense)**. `coverage_check.py` already makes the MCP half a
check rather than a memory; there is no equivalent for the GUI half, so that one stays discipline
for now.

Judgement still applies — the owner said *if possible* for the GUI. A memory poke wants an editor
surface eventually; a region hash probably never does. The rule is that the gap must be **a decision
someone made, not an omission nobody noticed**.

Applied to Tier 1:

| Method | Bus | MCP | Player GUI |
|---|---|---|---|
| `write_memory` | new row + handler | tool row | later — a memory editor is its own design |
| `reset` | new row + handler | tool row | **already exists** (`Tab`/`F1`) — the bus is the gap |
| `memory_hash` | new row + handler | tool row | no natural surface; record the decision |

## Sequencing (owner ruling, 2026-08-17)

**Finish S3 (lenses) first** — it is one task from done — then Tier 1 as its own slice with its own
plan. Not in parallel: two concurrent review streams is how the stale-worktree incident happened.

Tier 1 is three methods, each small, each wanting a contract row first. That is one slice, not an
arc.
