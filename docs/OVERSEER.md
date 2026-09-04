# OVERSEER.md — booting an oracle overseer session

> **Boot prompt (paste into a fresh session):**
> You are the oracle overseer. Read `docs/OVERSEER.md` in full, then the newest dated
> handoff/recon docs it names. You orchestrate subagents (dispatch → verify firsthand → merge);
> you do not implement directly. Work the queue top-down; keep this file current at merge windows.

Companion: the suite-wide protocol at `empyrean/docs/OVERSEER-PROTOCOL.md` (shared patterns; this
file is the oracle-specific half). Repo ground rules: the workspace `CLAUDE.md`. **Solo-first:**
everything below is workable with no peer sessions up — the queue, the follow-up register, and every
demand are committed artifacts in this repo; peers accelerate, they are never prerequisites.

## The role

Dispatch Opus subagents for implementation and recon; adjudicate contracts un-framed (a fresh
Fable agent, no steer) — **the seat is on HOLD, and that is a live owner ruling, not this seat's
call**: see owner ruling 2 below, and the substituted-reviewer rule under the 2026-08-27 hub ruling
for how adjudications run meanwhile. *(The 2026-08-22 provenance audit of the seat's original
ratification is closed and moot by that ruling's own words; moved verbatim to `OVERSEER-LOG.md`
2026-09-03. The standing rule it produced — never record an approval whose granting act you have
not seen — is live, and lives in the bars.)* Verify every gate firsthand before accepting a slice;
make the design rulings (delegated by the owner — pick best, record why); merge and push. The owner's standing
directives: **a legacy surface or demand spec is the compatibility floor, never the design
ceiling** (run a visible better-approach pass on every request), and **instrument co-development
with aeon** is the ratified lane (their diagnoses name gaps; we build them; the engine gets fixed
with tools that then exist).

## The boot read is bounded (100,000 B, gated)

Closed history is in **`docs/OVERSEER-LOG.md`**, not read at boot. Governing rule:
`origin/main:docs/OVERSEER-PROTOCOL.md`. **A live ruling goes here, never only in the log.**

## The queue (2026-08-19 end of day — reorder only with cause, record the cause)

*(Items 1-7 are closed and moved to the log. Item 8 keeps its live tail below; its closed sub-arcs
moved with them.)*

8. **▶ OPEN — THE ACCEPTANCE CONTRACT.** The definite list of what the successor must serve before it
   replaces the legacy C++ server. **Re-derive the membership, never transcribe it** — the machine-
   enforced source is `SCHEMATIZED_NOT_ADVERTISED` in `crates/oracle-aether/tests/schema_conformance.rs`,
   asserted as a whole sorted set, so it cannot drift silently. Board row `ACCEPT-16`. The arc's closed
   history (survey, CR-A, trio, CR-B) is in the log.

   **NEXT (not yet dispatched):** open from the survey and **not lost**: stale prose at
   `schema_conformance.rs:6,222` and the `resolve_target` `oneOf` divergence (**both folded into
   the in-flight `run_to_scanline` parcel** — remove from here once that lands), and a proposed
   **error-surface gate** — since no fragment declares error conditions, a suite validating only
   replies is blind to every error obligation.
   **FOREGROUND runtime follow-ups, never a subagent** (the emulator MCP deadlocks from background
   agents): the `step` frame-budget truncation (**STILL LIVE — needs a CR, not a probe**); ~~the `write_vram` SAT-cache desync~~ (**CLOSED 09-04**); and ~~**two new
   ones from CR-B** — the tail wrap (`z80_write {addr:"0x3FFC", bytes:<8>}` then read `$0000`;
   predicted from source: bytes 5–8 land at `$0000–$0003`) and the silent `len` clamp
   (`z80_read {len:10000}` → `8192`, no error)~~ ⚑ **BOTH CLOSED 2026-09-04 — AND THEY WERE NEVER
   OURS.** Measured firsthand against the binary a consumer spawns: both refuse LOUDLY and WHOLE,
   with a control proving the probe could see a write. CR-B was reading **`oracle-old`**; we did not
   serve the Z80 pair until `0f35ae1` (08-29), built to refuse exactly these. The booking never named
   an implementation. **`step` and `write_vram` above are UNTOUCHED and may carry the same defect —
   check which server each was read against before spending a probe.** Detail in the 2026-09-04
   register entry.
   ⚠ **ATTEMPTED 2026-08-22 evening and correctly
   ABANDONED, not deferred for convenience:** my MCP client found no socket in my own
   `$XDG_RUNTIME_DIR` (`Errno 2` — a *failing lookup*, which says nothing about the world, bar
   16(d)). A `pgrep` showed an emulator IS live, but it is **another lane's harness** with its own
   `XDG_RUNTIME_DIR=/tmp/oracle-harness-4av2i47x` running `aeon/s4.debug.bin`. Writing into a
   peer's harness Z80 RAM to demonstrate a wrap bug is the shared-machine hazard itself. These stay
   open until a lane-owned instance exists; **the CR does not depend on them** — it stands on the
   source read, and the runtime pass would only upgrade it from derived to demonstrated.
   **AEON OBLIGATION — SCOPE WAS WRONG, and the correction makes it bigger.** Item 7 recorded it
   as a dated heads-up before serving `emulator/wait_for_break`, because their gates send
   `timeout_ms`. **The survey found it covers THREE methods, not one, and I verified it firsthand
   at `origin/master` (not their working tree):** ⚠ **STALE AS OF 2026-09-04 AND IT COST A MIS-RANKING —
   RE-MEASURED AT THEIR `origin/master`: `raster_source_gate.py` has ZERO `wait_for_break` hits, and
   `snapshot_poison_gate.py`'s single hit is a COMMENT saying `emulator/run_to` replaced the arm/resume/wait
   triple.** The live call sites are `tools/evict_witness.py`, `tools/parallax_hscroll_probe.py`,
   `tools/raster_frame_epoch_probe.py` and the `aether_instance.py` client seam — none in the effects-gate
   lane. The original text below was true when written and is kept because a session that cites it must see
   that a verified-firsthand booking about a peer's tree still expired: ~~both scripts run an **arm → wait →
   clear** flow — `raster_source_gate.py:161/168/173` and `snapshot_poison_gate.py:62/64/68` call~~
   `emulator/breakpoint_add {addr}` → `emulator/wait_for_break {timeout_ms}` →
   `emulator/breakpoint_clear {all:true}`.
   **Consequence, and it is the load-bearing one: the migration CANNOT be piecemeal.** Serving
   `wait_for_break` alone would leave their flow with nothing to arm — so `wait_for_break` and the
   breakpoint trio ship as ONE parcel or the notice is worthless. The `timeout_ms` spelling was
   never the whole exposure; it was the part visible from a param grep.
   **This also gives the obligation a live reader BEFORE any date exists.** Their call sites bet on
   a specific breakpoint shape — `{addr: "0x…"}` to arm, `{all: true}` to clear, i.e. **address-
   keyed, no handles** — and **CR-A (D-13) is about to decide exactly that handle discipline.**
   Their input window is *now, before adjudication*, not when we ship. Note also
   `raster_source_gate.py:33`: under `deterministic=True` the legacy server answers `breakpoint_add`
   with a "det-mode stop" behaviour — a documented interaction our fragments say nothing about.
   The **date** still waits on the survey's pricing of that parcel; the **design consultation**
   does not, and holding it until a date existed would have consulted them after the ruling.
   If this session ends first, **the next one owes both**.


**Follow-up register** (each named where registered; deferrals here are unaudited estimates —
measured 3-for-3 cheaper than documented): F-SCANLINE-INDEX / F-SCANLINE-SH (priced down by the
sub-line arc), F-CRAMDOT, F-SUBLINE-{HGRID, ACCESSMCLK, DMASPREAD, CAPTURE-SCRATCH}, F-VCOUNT-PHASE,
the a2 B-2 gate gap (needs an H40/mode-switch fixture), F-HOSTED-RESET-SRM (**hosted reset bypasses
the player's .srm flush — warn clients off hosted reset until closed**), F-EQUATES-NAMESPACE,
F-CRAM-RAMP, F-PROF-TOTALS (superseded by delta 3), F-PALETTE-DRAG-PACE (evidence filed, rated
minor by its own filer), ~~stock-S1 symbols~~ (**CLOSED 2026-08-20** — the `|`-reader, the 48-bit
addresses, the forward-only equ ruling and the no-appendix binding all shipped; F-LST-AS-COLUMNS and
F-LST-NONDEB2-BINDING retire with it), **F-TICK-BOUNDARY-DIVERGENCE** (2026-08-20, from aeon's spike hunt, TICK-VARIANCE.md): over one
31-frame max-diagonal window on byte-identical ROM bytes, oracle-old runs 26 logic ticks where we
run 29 — exact agreement at the corpus-era state, one-tick difference at idle, divergence only
where a tick sits near the frame boundary: the two emulators disagree how much work fits in a
frame. Settling experiment (theirs): a single-tick trace at the first divergent boundary (frames
~7-8; states in TICK-VARIANCE §1.2) on both instruments. Corroborating fossil: the 2026-07-23
RT-3 finding — oracle-old OVER-drops ~8 startup ticks via `ClampHandshakeTimeDeterministic`'s
over-conservative bus-arb clamp, and ours was the tick-accurate side then too. Unresolved, not
urgent, CR-28-era sweep candidate. plus the Tier-1 carry-forwards in
`docs/2026-08-18-tier1-bus-methods.md`.

**Registered 2026-09-04, from taking the foreground runtime backlog the moment an instrument existed:**

- **▶ CLOSED — THE TWO CR-B Z80 RUNTIME FOLLOW-UPS WERE NEVER OUR DEFECTS, AND THE BOOKING IS WHAT WAS
  WRONG.** The acceptance-contract section booked two runtime demonstrations on 2026-08-22: the tail wrap
  (`z80_write {addr:"0x3FFC", bytes:<8>}` → *"bytes 5–8 land at `$0000–$0003`"*) and the silent `len` clamp
  (`z80_read {len:10000}` → `8192`, no error). **Both measured firsthand today against our own shipped
  binary through the consumer's own spawn path, and both REFUSE LOUDLY:**
  `-32004 the Z80 window is 0x0000-0x3FFF and this access ends at 0x00004004 — refused whole rather than
  wrapped, because a wrapped write lands on 0x0000 and reports success`, and
  `-32602 \`len\` = 10000 is outside 0..=8192`.
  **The refusal was verified WHOLE, not partial** — `$1FFC-$1FFF` and `$0000-$0007` byte-identical to
  baseline after the refused write — **and the probe was controlled**: a 4-byte write at `$3FFC` DID land at
  `$1FFC`, so the unchanged reading is a measurement and not a blind probe (bar 16(d): for an absence the
  control IS the measurement).
  ⚑ **WHY THE BOOKING WAS WRONG, and it is this file's OWN bar firing on this file's own register.** The
  entry never named an implementation. It was a claim about *"the server"* — and CR-B was reading
  **`oracle-old`**, at `d629771`, as `engine.rs:4630,4643`'s own doc comments say in as many words
  (*"the legacy server silently clamped 10000 to 8192"*; *"clobbered `$0000`, and replied success"*).
  **Our server did not serve `emulator/z80_read`/`z80_write` AT ALL until `0f35ae1` (2026-08-29)**, whose
  subject is literally *"bounded at both ends"* and which shipped a 228-line `tests/z80_window.rs`. So ours
  was **built to refuse these**, with the legacy behaviour named as the thing being avoided.
  **The register entry described legacy defects in words that read as ours, and a later session — this one —
  spent probes on it.** That is exactly the two-implementer conflation this file already books a sweep for
  (*"every claim about server behaviour either names its implementation or is a latent conflation"*), landing
  on the register that was written to track the conflation. **The sweep is now owed against this file's own
  follow-up register, not only against the recon/demand/CR docs it was scoped to.**
  ⚠ **Do not read this as wasted work.** Nothing before today had confirmed the `0f35ae1` guards hold **in
  the binary a consumer actually spawns** — the merged-serve bar's exact question, and the probes answer it
  firsthand. What was wasted was the *reason* for running them.

- **▶ ALSO CLOSED — `write_vram` SAT-CACHE DESYNC. Third of the four, same shape.** Resolved by the
  2026-08-27 `write_vram` parcel (`docs/2026-08-27-write-vram.md` §5) and never struck here. The survey's
  claim was TRUE of `vram_mut()` — a bare-array hatch with no SAT write-through — but **the bus does not use
  it**: `Vdp::poke_vram` was added with the same store and the same cached-half mirror, and
  `engine.rs:3626` calls it. **Verified live today rather than from the doc**: wrote `0x0100` over sprite 0's
  Y word at `satBase 0xB800`, and `emulator/sprites` reported `y: 128` with **`cacheDivergence: false`** —
  the desync would have shown the OLD `y: 104` and `cacheDivergence: true`. Restored byte-identical
  (`00E8 0501 A3F8 0118`, read back and compared). The anti-drift row `vram_poke_matches_the_port_path`
  (`vdp.rs:2780`) is what keeps the duplicated SAT arithmetic honest.

- **▶ STILL LIVE, AND DIFFERENT IN KIND — `step` FRAME-BUDGET TRUNCATION. Do not close it with the other
  three, and DO NOT SPEND A PROBE ON IT.** It is **not** a which-server defect: it is a **contract shape**
  gap that binds any conformant server. `step`'s `count` has no ceiling while every advance primitive is
  frame-bounded (`max_run_frames` default 3600), so an over-large `count` stops early and **the fragment
  gives the reply no key to say so** — no `stepped`, no `reached`, and `caveat` declared ABSENT, so emitting
  one would fail §8 item 20's closure. The shortfall is visible only on `emulator/stopped`'s
  `deadlineReached`.
  **It needs a CR — `stepped` in the result, or a `count` ceiling, or permission to carry `caveat`; any one
  closes it, the current row closes none — and a demonstration adds nothing a CR reader needs.**
  ⚑ **Why THIS one survived honestly while the other three rotted: it is documented AT THE HANDLER**
  (`engine.rs:3040-3053`, in `step`'s own doc comment, naming the CR it argues for). The three stale ones
  lived only in this register. **A perishable claim decays where nobody re-reads it; a claim beside the code
  it describes is met by everyone who touches that code.** That cuts directly against this file's own
  standing bar that the worst place for a perishable claim is a code comment — **both are true, and the
  distinguishing variable is whether the claim is about the code it sits beside.** A comment describing its
  own function is read by the next editor; a comment citing a ruling made elsewhere is not.

- **▶ AND THE SHAPE THAT MADE IT SURVIVE: A BOOKED DEFECT CAN BE CLOSED BY UNRELATED WORK, AND NOTHING
  CLOSES THE BOOKING.** The guards landed 08-29 inside a parcel serving the Z80 pair; the follow-ups sat in
  this register as open for six days afterwards, through several sessions that read this file at boot.
  **A register entry has no reader who would meet the contradiction** — the code moved, the tests went green,
  and the only artifact still asserting the defect was the one nobody re-derives. Same family as the queue-row
  bar (*a row's justification ages like a precedent narrative*), one level lower and worse, because a
  follow-up register is read as a to-do list rather than as a claim. **When a parcel serves a method, grep
  this register for that method's name before writing the landing.**

**Registered 2026-09-04, from the WAITFORBREAK landing:**

- **F-HANDSHAKE-LOAD-TIMEOUT** — `tests/handshake.rs::initialize_advertises_a_generated_method_list_that_is_the_dispatch_table`
  fails with a socket read timeout (`WouldBlock`, `tests/common/mod.rs:196`) under load average ~250.
  **The implementing agent's measurement, not re-derived here** (3/3 on their branch, 2/2 on `main`'s
  `oracle-aether`, green 15/15 at normal load), so it is attributed rather than asserted. Same CLASS as the
  wait_for_break defect just fixed — a suite row that only fails under peer load — and a **different root
  cause**; do not assume the fix reached it. Booked because this repo has now twice written off a
  load-sensitive row as a flake and been wrong: the wait_for_break row was tagged a flake on 2026-09-03 and
  again on 2026-09-04 before it was root-caused. **A row that only fails under load is a defect with a
  narrow window, not a flake, until something says otherwise.**

- **F-TMP-RESIDUE — DID NOT REPRODUCE, and the disagreement is the point.** The same agent reported 9,675
  `/tmp/oracle_config_save_load_*_ThreadId(N)` directories, oldest 2026-08-26, leaking from outside this repo.
  **Re-measured here minutes after their run: FOUR.** `grep -rn oracle_config_save_load crates/` returns 0
  here, and a grep across aeon/aurora/seraph/sigil/empyrean/oracle-old returns no file at all, so the name is
  attributable to nothing in the suite; `/tmp` is at 60%. Either the count was wrong or something reaped them
  between the two reads, and **nothing available now can distinguish those** — which is why it was not
  relayed to the hub as a shared-machine hazard. Recorded as a caught relay, not as a finding.

- **F-SPAWN-PICKER-PANEL-SURFACE — the owner's words name a surface that has no pointer at all, and the
  parcel landed on the OTHER one. Booked so the gap is a decision, not an omission.** His tab ruling says
  spawn's surface is *"clicking a spot in the Screen panel"*. **There are two windows**: `oracle-frontend`
  (minifb, the game window, where `pick.rs`'s click-to-watch and `present::window_to_native` already live)
  and `oracle-player` (egui, the debug tabs — Registers/Memory/Objects/Screen/nav). *"Screen panel"* is
  `oracle-player`'s tab. **Measured: `crates/oracle-player/src/screen.rs` (541 lines) has ZERO pointer
  interaction** — its one `click` hit is the word inside a doc comment; the crate's only `clicked()` calls
  are buttons in `ui.rs`/`nav.rs`. So the surface his sentence names cannot receive a click today.
  **SPAWN-PICKER (merge `531894e`) landed on `oracle-frontend`**, which is where the gesture exists and
  where every artifact this seat's own brief cited actually lives — **the brief conflated the two windows,
  and the agent caught it rather than half-building across the seam.** That refusal was correct: the panels
  surface needs an egui-rect→native-dot mapping invented from scratch plus its own standing indicator.
  **Not a defect in what shipped; a second surface.** Per this lane's three-surface rule the gap must be a
  decision, so it is one. ⚑ **Needs ONE WORD FROM THE OWNER, filed in `awaiting`: which window did he mean?**
  If the game window, this is closed today. If the panels window, it is a fresh parcel.

- **F-SHIM-SOCKDIR-RESIDUE — the PROCESS half did not reproduce; the FILESYSTEM half did, and it is the
  real finding.** aeon relayed (via the hub, ~08:20Z 2026-09-04) 13 leaked `oracle-aether` processes on
  `/tmp/oracle-mcp-*` sockets, oldest 2026-08-28, ~38 MB. **Re-measured here minutes later: ZERO
  processes** — `ps -C oracle-aether` empty, `pgrep -x` 0, against a working control (`pgrep -x zsh` = 20),
  so the empty result is a measurement and not a broken pattern (bar 16(d)). The one `pgrep -f` hit was
  **this lane's own subagent mid-build** and was gone on the next command: reaping by pattern would have
  killed our own in-flight parcel, which is the shared-machine hazard arriving from the direction nobody
  warns about. **Second relayed count in one day that did not reproduce** ([[F-TMP-RESIDUE]] was 9,675 vs 4).
  ⚑ **But the artifact proves a real gap the process count was standing in for: 50 `/tmp/oracle-mcp-*`
  dirs, 50 socket files, ZERO listeners (`ss -lxp`), spanning 2026-08-27 to 2026-09-03.** Nothing reaps a
  shim's socket dir when its child dies. Disk cost is **200 K total**, not 38 MB — that figure was
  process RSS, a different quantity, so the two reads are not in conflict about the same thing.
  **Ruled: nothing to reap** (a 200 K write against another lane's possible live path is the wrong trade),
  **and the shim-reaps-on-disconnect question is booked, not fixed** — the shim is
  `oracle-old/linux-port/mcp/oracle_mcp.py`, and `oracle-old` is reference-only with the cutover existing
  to delete it. Revival: the cutover replacing the shim, or `/tmp` pressure becoming real.
  ⚠ **Ops fact for this session, and it is the vintage bar pointing at us:** our own `mcp__oracle__*`
  server DISCONNECTED during this measurement — consistent with the same reap. Foreground runtime
  follow-ups are unavailable this session until a relaunch; a `/clear` does not fix it.

**Registered 2026-09-04, from aeon answering the CR-J §11 flag:**

- **CR-J §11 UNMEASURED FLAG — CLOSED, MEASURED, both halves.** aeon verified their side statically at
  their `origin/master`: `Obj_Req_X/Y` on spawn go to `Load_Object` as *"integer, engine coords"*; on move
  through `pixels_to_coord` into 16.16 `Sst.x_pos`; `Warp_Req_X/Y` clamp against `Player_Bound_Right` in the
  same integer world-pixel space. **Ours measured live on `aeon/s4.debug.bin` at frame 240**, through a
  channel neither lane authored — the SAT the emulated 68000 wrote: `object_list` slot 0 at world (256,256),
  `Camera_X` (`0xFFA728`) = 96, `Camera_Y` (`0xFFA72C`) = 144, predicted dot (160,112); **SAT sprite 0 is a
  2×2-cell sprite at (152,104), centre exactly (160,112)**, and `pixel_attribution(160,112)` names sprite 0
  the winner. Static half read at `ba909f1`, `engine.rs:4742-4753`: `world = Camera_X + dot.x`, unbiased,
  symbols resolved per call. **Anti-vacuity clause, and it is why the run counts: the camera was at (96,144),
  not the origin** — a camera-space error would have shown; at (0,0) the identity is vacuous.
  ⚠ **One asymmetry told to aeon rather than left implicit:** we read the camera as **unsigned** u16 and add
  as u32, so we cannot express a negative world coordinate. Since their object path deliberately does NOT
  clamp, an out-of-act click reaches them as a large positive, never as something obviously wrong.
  The frontend comment at `bus.rs:405-408` still carries the flag as open and is now stale — fix it in the
  next parcel that opens that file.

- **▶ F-SPAWN-OUTSIDE-ACT — a real gap in SPAWN-PICKER as shipped, found by the peer we asked, not by us.**
  aeon: the warp path clamps and **the object path deliberately does not**, because an out-of-act object is
  culled by `RunObjects`' camera-distance test and does nothing, where an out-of-act *player* would reach
  `SEC_VOID`. **Consequence for the picker: a click outside the act is acked as placed and the object is
  silently culled** — no error, no refusal, nothing on screen. That is precisely the failure class the whole
  refusal design exists against (*a refusal arrives as a sentence*), arriving through the one path that
  returns success. **The fix is OURS, not a mailbox change**: we hold the click, so we refuse or clamp before
  sending. The refusal test needs the act bounds by symbol.
  **ACT BOUNDS — ANSWERED BY AEON BEFORE THE ASK WAS FILED, and their answer contains the trap.**
  ⚠ **DO NOT USE `Player_Bound_Right`/`Player_Bound_Bottom` AS ACT BOUNDS.** They are the PLAYER's clamp
  edges and are **inset** (`level_width − PBOUND_RIGHT_MARGIN`, `level_height − SCREEN_HEIGHT`), and objects
  are deliberately unclamped — so an object between `Player_Bound_Right` and the true `level_width` is
  **legal and renders**. Refusing there would refuse legitimate placements **and look right**, because the
  refusals would cluster at the edge where a person half-expects them. It is the symbol a grep for "the
  bounds" finds first (the warp path clamps against it). There is also **no `Player_Bound_Left`/`_Top`** —
  the low edge is a literal `0` in `clamp_and_publish`, so half the box has no name at all.
  **The class, sharper than this file's existing name-is-not-behaviour bar: the most dangerous wrong symbol
  is the one whose wrongness is SHAPED LIKE CORRECTNESS.** A wrong answer that fails where failures are
  expected reads as the feature working.
  **The real quantity:** `level_width = Act.grid_w << SECTION_SIZE_SHIFT` (=11, ×2048), same for height;
  valid box `[0,w) × [0,h)`. Reaching it live means `Current_Act_Ptr`, **which aeon's own source calls
  flaky** and which they declined to hand over as an interface on that basis.
  ▶ **RULED, and filed with them: aeon publishes `Level_Width`/`Level_Height` as two derived RAM words in
  `Player_BoundsInit`** (which already computes both before subtracting the margins). Resolved BY NAME per
  call, exactly as `Camera_X` is. Chosen over the pointer chase because the pointer would make our
  correctness depend on a cell they distrust **and the failure would be invisible from our side** — a bad
  pointer yields a plausible box and our refusal then passes on garbage. **Symbol absence is loud; a bad
  pointer is not.**
  **⚑ MEGA-ACT CEILING — recorded because it names which side cracks first.** `level_width`/`level_height`
  word-wrap above `$FFFF` px (grid > 31 sections), held today by a build-time `ensure` capping the grid at
  `$8000`. That `ensure` is what makes our **unsigned u16** camera read safe — a property of the current
  constraint, not of the design. If it is relaxed for a mega-act, **aeon's word stores break before our u32
  arithmetic does.** Nothing to do; written down so a future session here does not find the dependency by
  having it break.

**Registered 2026-08-29, from aurora's relay of the owner's R8 question:**

- **F-R8-LATE-REVISION** — a *copy-of-column-0* toggle for the R8 leftmost-partial-column quirk, so a
  fix can be seen under both hardware behaviours. **Declined as a booking, and the reason is not
  "noise".** Under the later behaviour the leftmost column takes column 0's vscroll, so the defect
  simply **disappears** and aeon's column-19 write becomes an inert no-op rather than something the
  differential validates — the whole verdict is derivable without building it. Against that we have
  **no hardware-tested rule for the late revision**, only Plutiedev/Stef descriptions, where the early
  rule pinned at `render.rs` `plane_vscroll` (H40 `VSRAM[$4C] & VSRAM[$4E]`, H32 `0`, same value both
  planes) matches Genesis Plus GX's *"verified on PAL MD2"*. Shipping a second model whose fidelity
  cannot be established, and then letting a consumer validate a fix against it, is bar 9's corollary
  exactly: an unvalidated instrument adopted as a gate returns a **confident wrong verdict**.
  *Revival condition:* a hardware-tested rule for the later revision appears, **or** a second scene
  turns up where the fork changes a design decision. The divergence ledger already records the fork,
  so nothing is hidden meanwhile.

- **F-BANNER-INVITES-A-PIN** — *found from the other end, by a consumer breaking on it (aurora's O26,
  2026-08-29).* Our startup banner prints `aether: N methods advertised`, and `Bus::start` prints the same
  total on the serving line. **A published total is an invitation to pin it**, and a consumer did: their
  `classic-playtest-harness.mjs:171` pinned `methods === '35'` and *threw* `stale oracle-aether binary` on
  anything else — so the guard written to detect staleness became the stale thing and rejected every
  correct binary. **Measured firsthand at `6031020`, two ways: the banner says 52, and `initialize`'s
  `methods` array has length 52** (spawned and called, not read off a schema).
  **The defect is theirs; the surface that manufactures it is ours.** A count changes for reasons unrelated
  to freshness and is identical across binaries that differ, so it is the wrong observable for the question
  every consumer actually asks — *is this binary current?* We already serve the right answer and do not point
  at it: `initialize.serverBuild` carries `{id: "<sha>+profile=…+target=…+features=…", source, dirty}`.
  **Cheap fix, and it is a documentation-and-adjacency fix, not a removal:** name `serverBuild` in the same
  breath as the count, so the number a reader meets first is not the only identity on offer. Do **not**
  simply delete the total — it is genuinely useful at a glance, and aurora's lesson is about what a consumer
  should *key on*, not about what we may print. Our own side is clean: grepped, every use is `METHODS.len()`
  and no literal total is pinned anywhere (`crates/oracle-aether/tests/params_closure.rs` closes over
  `METHODS.len()`, which is the derived form).
  **The durable line, aurora's: a total was the wrong observable.** Same family as this file's own
  name-is-not-behaviour bar — a number that *correlates* with the property being tested, standing in for the
  property, and reading exactly like a real check until the correlation breaks.

  ⚠ **AMENDED SAME DAY, and the amendment corrects THIS BOOKING, not the consumer** *(aurora, 2026-08-29,
  who class-checked the SHA out of habit and found the thing I had just recommended people point at)*. The
  paragraph above says "name `serverBuild` in the same breath as the count". **That advice is incomplete in
  a way that walks a consumer back into the same trap from the other side.** `serverBuild.id` names whatever
  HEAD was at build time — and the id they measured, `6031020`, is a **docs-only commit** (`lane-status.json`,
  +10/−18). That is correct behaviour for a build identity and is exactly what staleness wants, but it means
  **the id moves for reasons that have nothing to do with the code**: binaries built at `acf41f5` and at
  `6031020` contain identical code and report different ids.
  **So the field answers *"is this the same binary I measured before?"* and MUST NOT be compared for equality
  to answer *"does this build contain feature X?"*** — that second question belongs to `capabilities` and
  `methods` membership, which is the derived form this register already recommends. Whenever we point a
  consumer at `serverBuild`, we owe them that sentence in the same breath; a pinnable identifier offered as
  the cure for a pinnable count is the same defect in better clothes.

- **F-SERVERNAME-PREDATES-THE-RENAME** — `EngineConfig::default()` sets `server_name: "oracle-next"`
  (`crates/oracle-aether/src/engine.rs:205`, read at `fee8f12`), so every `initialize` still answers with the
  **pre-rename** repo name; `serverVersion` is `"0.0.0"`. Spotted by aurora 2026-08-29 while assessing what on
  our wire is an identity.
  **Not a wire-correctness bug, and say so plainly:** §2.1 deliberately demotes `serverName` to a *deployment
  label* and moves identity to `implementation` (`"oracle-rs"`) and `serverBuild`, both read from `build_info`
  and — verified firsthand, not from the comment — **barred from configuration by a source-level test**
  (`tests/server_build.rs::neither_identity_value_is_reachable_from_configuration`). A consumer reading
  `serverName` for identity is reading the field the contract told it not to.
  **But the value is still a stale name we publish on every handshake**, and "it is only a label" is exactly
  how a wrong string survives a rename. **Changing it is wire-visible**, so it does not get a drive-by edit:
  it needs bar 14's consumer-set enumeration first — grep every sibling tree for the literal `oracle-next`
  with real client context — because the failure mode of a consumer matching on it is silent. Revival
  condition: do it as part of any deliberate handshake pass, never alone.
  **ONE CONSUMER CLEARED, AND THEIR OWN CAVEAT IS THE REASON IT IS STILL NOT A GREEN LIGHT** *(aurora,
  2026-08-29, asked for exactly this input)*: they grepped `src/`, `test/` and their harnesses — 40
  references to `serverName`, **zero** comparisons/branches/`includes`/`startsWith`/`match` on its value;
  every use is display or pass-through. **But they also volunteered that the literal `'oracle-next'` appears
  21 times across 8 of their test files as fixture INPUT**, with assertions derived from the payload
  (`expect(s.serverName).toBe(PAYLOAD.serverName)`) — so a rename leaves their suite green **while their
  fixtures quietly describe a server that does not exist.** Their formulation, worth keeping verbatim in
  spirit: *our green is not evidence your rename is safe; it is evidence we do not look.* That is the
  clearest statement of this register's own no-consumer-broke hazard anyone has offered, and it came from
  the consumer. **Four repos remain unenumerated** (aeon, seraph, sigil, empyrean), so the booking stands;
  aurora has asked to be told if it moves, so they can re-point their fixtures.

- **F-RSP-XVFB-ORPHAN** — *audited into existence by a peer's warning, and the audit came back clean on
  the thing they warned about.* aurora relayed their O16 finding 2026-08-29: 28 of their harnesses tore
  down with `pkill -f '<dist path>'`, an argv pattern that matched **other sessions' processes** and had
  killed a peer's Electron mid-run three times. **It does not apply here, and that is a measurement, not
  an assumption.** Enumerated by what *touches process teardown* rather than by the token (protocol bar
  8): the executable surface is 16 `.sh`/`.py` files plus every `.rs`; spawn sites are five
  (`rsp.py:43`, and four `Command::new("git")` that are `output()`-style and never outlive the call);
  **teardown sites are exactly one — `rsp.py:157 self.p.kill()`, on the `Popen` handle that object
  itself spawned.** Ownership by construction: no pattern, no process name. Every `pkill`/`killall`
  string in this repo is in **docs prose warning against it** (5 in `docs/`, plus stale copies inside
  three dead `.claude/worktrees/`), zero in code. The control run (165 bare `kill` hits) confirms the
  grep could see what was there, so the empty result is a measurement rather than a broken pattern.
  *(Residue, aurora's strengthening and the scope notes: `OVERSEER-LOG.md`. Revival: the differential harness run in anger again, or a stray blastem/Xvfb outliving it -- fix is a process group, not a wider pattern.)*

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 233-289.)*
BOTH OF TONIGHT'S FAILURES WERE ON THE EMITTING SIDE, WHERE NO RULE REACHES.** aeon's formulation, banked
by them at aeon `4fae2d8d`; two instances, one from each lane, hours apart.


**▶ F-CR28-CALLERS-DANGLING, registered 2026-08-30 — an unmerged commit in a leftover worktree, found

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 294-323.)*
while earning an `atBoundary: true` claim rather than asserting one.**


**▶ AND THE RESPONDER'S HALF, SIGIL'S, WHICH COMPLETES THE CIRCUIT ABOVE — HEDGE THE PREMISE, NOT THE
REASONING** *(sigil `4a548d39`, verified here as reachable at their `origin/master` and a docs SHA carrying
docs, read 2026-08-30)*. Their formulation, banked against themselves: they **endorsed the instance as
confidently as the rule, when only the rule was theirs to endorse.** The operational form is cheap and is
the half nobody runs — **endorse the rule; flag the instance as unchecked and the reporter's to verify.**
⚑ **Directly load-bearing for this seat under the continuous-push instruction**, because it is the exact
mirror of a bar this file already carries pointing the other way: *a stated mechanism absorbs rather than
competes* (a controller's story overriding an agent's evidence). Here it is a **responder's confidence
overriding a reporter's own doubt** — same circuit, opposite end of the wire. This lane held only the half
that flattered it, and so did they.
**Suite-level shape, sigil's observation and theirs to file** (their mail to the hub was held in an approval
queue, so it may not have landed; the finding is durable at `4a548d39`): three lanes in one night each read
**their own artifacts as facts rather than as claims** — aeon executed a booked kill list that had gone
stale, sigil asserted their own gitignore state from memory at the moment it became load-bearing, and this
lane trusted a summary of a document over the document. **Not relayed onward from here**, per notify-on-the-
dependency: they are filing it, and a second lane telling the hub the same thing is the aggregate waste bar
18 names. Recorded so the pointer survives if their mail did not — it lands on the hub's own live
`PLAN-PROSE-SWEEP` item.

**▶ REGISTERED 2026-08-30 — F-LEGACY-SILENT-DEFAULT, and it is the sharpest argument the cutover has.**
*(Measurement history in `OVERSEER-LOG.md`.)*
**Revival condition:** the README sentence owner ruling 4 requires — it should carry this fact, not
merely that the surface is legacy. Explicitly NOT a fix recommendation for `oracle-old`: it is
reference-only and the cutover exists to delete it.


## ⚑ OWNER RULING — PUSH AUTHORIZATION — ✅ **CONFIRMED DIRECTLY BY THE OWNER, 2026-08-24, IN THIS SESSION**

**STANDING APPROVAL, OWN REPO ONLY: a lane may push its own repo's master without asking each
time.** Reached us via empyrean-18, banked by the hub at empyrean `2bd72a03` — **verified firsthand
here: the object exists, is an ancestor of their `origin/main`, and is a docs commit carrying a docs
ruling, so its SHA class matches what it anchors.** Flagged as a relay per this lane's own standing
rule; ~~direct owner confirmation requested in-session.~~
**✅ THE CONFIRMATION ARRIVED, AND THE FLAG IS REPLACED RATHER THAN DELETED, per the rule that wrote
it.** The owner answered decision `d-1` directly in this session on 2026-08-24, choosing **"Confirm it
as standing permission"** from the two options put to him. **This lane may now push its own repo's
master without asking each time**, under the four conditions below, which ride with the grant and are
unchanged by the confirmation.

**The granting act is named, which is why this relay is usable at all.** The hub consolidated a
question two lanes had stopped on separately (sigil asked outright; aeon was sitting on three
finished docs commits for the same reason, neither able to see the other asking), put three options
to him — own-repo standing / standing-for-docs-ask-for-code / per-push — and he chose the widest.
That is a granting act described, not a status field quoted, which is the distinction the
never-record-an-unwitnessed-approval bar exists to draw.

**The conditions ride with the grant and are part of it**, transcribed rather than paraphrased:
- **verify `origin` actually moved — the push is not the act, the remote moving is.** This is the
  protocol's own push-before-you-cite rule arriving as an owner condition;
- **never rewrite already-pushed history**;
- **never push another lane's repo**;
- **publication to the public wiki site stays a separate explicit ask** — not a concern in this
  tree today, but it becomes one the moment the wiki-emulator spike produces anything shippable.

**Scope, stated by the hub because this is the class of grant that gets restated wider: it
authorizes PUSHING, not the work being pushed.** It does not release this lane's boot stop, it is
not approval to dispatch or to land a parcel, and **it does not touch the CR-A/CR-B adjudication
hold**, which remains a separately parked owner item.

## ⚑ OWNER RULING, 2026-09-02T18:20:42Z — CUT THE CEREMONY. **IT OUTRANKS EVERY BAR IN THIS FILE.**

Verified at empyrean **`90554f2`**. ⚠ **Relayed here 09-02 and never banked — zero occurrences in this
file or the log until 09-03, so the 09-03 session spent hours on apparatus it forbids. The absence is the
failure.**

Owner, asked *"did we do something beurocratic to slow things down?"*: *"Yes please cut anyything that's
arbitrarily slowing us down without like an actual good reason please … as long as it's correct and stuff
and hitting our goal, that should be what we mainly care about."*

In force until EFFECTS-W1 ships:
* **No new process bars, no rulings about rules, no boot-doc growth.** New bars go to
  `docs/OVERSEER-PENDING-BARS.md` PARKED, not into force. The protocol pass waits.
* **A correction is ONE LINE in the lane log. No story.**
* **DoD items and the bug tier only** — no cross-lane audits, no instrument or ledger work, no re-measuring
  a peer's numbers, unless it blocks a DoD item or ships wrong output.
* **Status files, decision cards and lane logs are written once in the accepted shape and not polished.**
* **The boot-read gate stays, but nobody hand-trims for it** — over the bound, move history out in ONE cut
  and carry on.
* **"Correct" is unchanged:** a landing still builds, passes the lane's own tests, and shows on screen or
  in a witness. What is cut is certifying things that are not the feature, and record-keeping about the
  record-keeping.

## ⚑ OWNER RULING, 2026-09-03T05:21:01Z — **REPORT TO THE HUB WHENEVER YOU FINISH OR STOP**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the relayed
rulings below. **Verified firsthand rather than taken on the relay's word**, which is what makes it
usable: empyrean **`f04afe3`** is an ancestor of their `origin/main`, `--stat` shows it is a docs commit
carrying a docs ruling (so its SHA class matches what it anchors), and the owner's words are present in
the blob at that revision. His words: *"tell the agents any time theyy finish work or stop to report to
you please, loosk like aeon's stopped right now"*.

**Standing, every lane: a landing, a boundary, a block, an owner question, or a dispatched agent
returning — anything that leaves nothing running — gets ONE message to the hub saying what landed (SHA
emitted from git output, never typed) or why you stopped, and what you need.** Going quiet with nothing
running is the state he named.

⚠ **This does NOT license the aggregate waste bar 18 exists for.** The trigger is *finishing or
stopping*, not *changing something* — a pin, a correction, or an interesting finding still needs a named
reader before it is sent. The two rules compose: report your own state unconditionally; relay a *fact* to
a peer only when you can name their dependency on it.

## ⚑ FOUR OWNER RULINGS, 2026-08-22 — **RELAYED, NOT WITNESSED BY THIS LANE**

Reached us via empyrean-73, quoting the owner's own words in their session. **Flagged as a relay
per this repo's own rule** (*never record an approval whose granting act you have not seen*) — which
was written earlier the same day, after that shape failed twice across two lanes. Quoted words with
a named source are far stronger than a status field and are still not a witnessed act. **Direct
owner confirmation requested in-session; replace this flag with the confirmation, do not delete it.**

1. **Wiki-emulator spike: APPROVED, and my flagged divergence resolved — in the direction that
   corrects empyrean, not us.** The approval was **real all along**; the spec's self-declared
   "Approved design" was factually correct, and empyrean's correction to me was wrong on the fact.
   ⚠ **Keep both halves:** an unverifiable claim turning out true does **not** retroactively make
   recording it without a citation correct — *we got away with one.* Owner: *"Yes I did but I was
   trying to save fable use so I never had an agent start. I can now if it wants with opus but just
   be careful and if we get stuck don't push."* **Authorised on Opus, with two conditions in his own
   words** — a spike, not a commitment: **report the wall rather than engineering around it.**
   Escalate to empyrean rather than burning a week proving feasibility that was meant to be cheap.
   **Not reprioritised above the acceptance parcels.**
2. **Fable seat: HOLD — with a new obligation that is better than either option I offered.**
   ⚠ **CLOSED AS AN OWNER ITEM 2026-08-22 — STOP LISTING IT AS PARKED.** Asked whether to fund the
   seat long-term he answered *"Idk what you want for this"*, and the asking lane recorded that as
   **their badly-formed question, not his indecision** — the right way round. **No decision is needed
   today: hold stands, the ledger IS the mechanism, and the question returns naturally when the limit
   lifts.** Note this also retires the provenance worry above by superseding it: there is now a live
   cited ruling on the seat, so the unwitnessed 2026-08-21 ratification is **correct and moot**.
   Owner:
   *"keep careful record of what's done without fable so when our limit is no longer up the first
   thing it can do is make sure we made the correct decisions without it."* **Fable's FIRST job when
   the limit lifts is auditing exactly those decisions**, so the gap becomes a queue rather than a
   hole. ▶ **Ledger created: `docs/2026-08-22-unadjudicated-decision-ledger.md`** (L-01…L-06). Each
   entry must be adjudicable **cold** — verdict, alternatives, evidence at the time, and *what would
   have to be true for it to be wrong.* An entry recording only the verdict is useless to the audit
   it exists for. **Every future unadjudicated call gets an entry at the moment it is made**, not
   reconstructed later. Note: this supersedes the unwitnessed 2026-08-21 ratification audited above
   — there is now a live cited ruling on the seat, so that correction stands as *correct and
   superseded*.
3. **▶ THE MOST CONSEQUENTIAL, and it is aimed at this lane.** Owner: *"Oracle - let's make sure
   anything not going for the new oracle does and tell it to make sure to tell the oracle agent to
   build out any tools these other suite items/agents might need, that's how we're getting robust."*
   Two halves: **(a)** anything still pointed at the legacy C++ server should be moving to the new
   core — **the acceptance contract is the vehicle and is effectively blessed as the priority**;
   **(b) this lane is the SUITE'S TOOL-BUILDER.** empyrean is telling every lane to send named
   instrument asks here rather than working around gaps. **Inbound capability asks are first-class
   queue items, not interruptions** — his stated reason is *"that's how we're getting robust"*. This
   extends the existing aeon co-development lane from one peer to all of them.
4. **READMEs: make every suite repo's README accurate.** *"Doesn't have to be super in depth."*
   Ours must say plainly that **the MCP surface still reaches the legacy C++ server** — the fact
   most likely to mislead a reader, and the one this lane independently flagged in the status
   roll-up before the directive arrived.

> ▶ **BOOTING INTO THE CUTOVER? READ `docs/2026-08-22-cutover-handoff.md` FIRST.** It is written for
> the session that boots *after* the owner flips the config and relaunches every lane — what the
> rebuilt binary at `12cc17e` guarantees, the 17 remaining, why a `-32601` is a success signal, and
> what to do first when lanes report gaps. This section is the record; that file is the instructions.

## ⚑ HUB RULING UNDER DELEGATION, 2026-08-27 — d-16 SUBSTITUTE: the reviewer seat, and the rule it creates

⚑ **RELAYED, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the four rulings above. The
owner armed an overnight delegation in his own words (*"if anything needs decision that they can't
make you make it for them"*, transcribed by the hub into empyrean `OVERSEER.md` addition (f) at
05:39Z, banked `091ac59`) and went to bed; the hub ruled in his place and **he reviews it on return.**
Record it as the hub's ruling. Do not upgrade it to his.

**The question (d-16):** the premium independent-reviewer seat was parked days ago when it blocked
nothing. It had come to block three items — OVERLAY-STATE and CR-A, five of the sixteen unserved
methods among them. **Ruled: SUBSTITUTE.**

**THE STANDING RULE THIS CREATES, and it outlives tonight.** Adjudications run on the ordinary model
while the seat is parked, and **every ruling produced that way NAMES ITS OWN REVIEWER, at the top, in
the ruling itself.** Not in a covering note, not in the dispatch record — in the artifact a later
reader picks up cold. The reason is the whole design: Fable's first job when the owner lifts the limit
is auditing exactly these, and an audit cannot find what does not announce itself. **Independence is
preserved — a fresh reviewer that took no part in the drafting is still the half that catches real
problems. Reviewer tier is what was spent.** Say it that way; do not describe a substituted ruling as
adjudicated without qualification.

**Ledger:** every substituted adjudication gets an entry in
`docs/2026-08-22-unadjudicated-decision-ledger.md` **at the moment it is dispatched**, not
reconstructed after — the entry must be adjudicable cold, and must name *what the audit should re-run*
and *what would have to be true for the ruling to be wrong*. First entry is **L-07 (CR-A)**, which also
records the cheap first cut for the audit: **re-run the material items only**, since the M/S split
every ruling here is required to produce is the instrument that measures what the substitution cost.

⚠ **Numbering collision, live:** aeon also has a card numbered `d-16` (background chunk height). The
console shows two. **Never cross-reference a decision by number alone across lanes** — say the lane.

## ⚑ THE CUTOVER — ruled 2026-08-22 (RELAYED, see the flag above), mechanism determined firsthand

**The ruling** (owner, via empyrean, quoted): *"I say do it now and when something is needed have it
built out, no?"* — cut `mcp__oracle__*` over to the Rust server **now**, and close the remaining
methods **on demand** rather than in enumeration order. empyrean recommended registering alongside
the legacy server; **he overruled it** and they now agree, as do I: it converts the acceptance
contract from a catalogue into demand-driven work.

**✅ RULED PROCEED (relayed, with the full measured cost disclosed to him first — aeon's two gates
down ~a day, Z80 no real consumers, binary needs rebuilding). His words:** *"Yeah just proceed. We fix
when we come across it, if we don't we build later but this is really just to start building out the
tooling."*
**⚑ THE LAST CLAUSE IS THE GOVERNING ONE, AND IT REFRAMES THE WHOLE ACCEPTANCE CONTRACT.** The cutover
is **not** happening because the successor is ready — it is happening **because being reachable is what
generates the demand that builds it out.** So the remaining 17 are **not a checklist to burn down
before the switch; they are a queue the switch POPULATES in priority order.**
**▶ CONSEQUENCE FOR EVERY BRIEF FROM HERE: an early gap is a SUCCESS SIGNAL, not an embarrassment.**

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 565-566.)*
State it explicitly to agents — the instinct will be to treat every `-32601` as a failure that should
have been prevented, and under this ruling it is the mechanism working. **This does NOT relax the
loud-failure requirement**; it is the reason for it. A gap that refuses by name feeds the queue; a gap
that degrades to a plausible answer poisons it.


**▶ ALSO OPEN, and it points INWARD at our own docs (aurora ran it on theirs and it drew blood).**
Their split: **engine facts** (properties of aeon, seen through a window — unaffected) vs **server
facts** (properties of ONE implementation). Their worked example is the one to internalise: they
re-derived the `require_paused` set **from our Rust source**, banked it as a correction — and wrote it
into a section that had always called these properties of *"the bus"*. **It is a property of one
implementation and the correction did not say so** — a defect created *while applying every other bar
correctly, in the act of fixing a different staleness.* **We have the same exposure and more of it:**
this repo's recon, demand and CR docs describe "the server" throughout, and D-10/D-13/D-17 are already
booked as having **two implementers**. A sweep is owed — every claim about server behaviour either
names its implementation or is a latent two-implementer conflation.
*Durable formulation from the same thread, worth more than its instance:* **freshness is not
transitive across a document, and proximity reads as verification** — a stale figure beside a
freshly-updated one is read as cross-checked, which is how my own 37 survived hours next to a correct
18.

## ⚑ THE SOCKET CHAIN, AND F-CHAIN-QUOTED

**There is no chain.** `empyrean/clients/python/aether.py`'s resolver commits on a *directory* test, so
every lane resolves to `$XDG_RUNTIME_DIR/oracle.sock` and stops; `/tmp/oracle.sock` is unreachable dead
code. The spec specifies a chain and the reference client never implemented one — a conformance gap,
not a stale comment. **Operational consequence: start a server on `/run/user/1000/oracle.sock`.**
**F-CHAIN-QUOTED stands:** two historical recon docs here name the socket paths and neither was written
from the resolver. Revival: before any doc here is cited to a peer as the transport's behaviour.
Full measurement, and what each of the three lanes had right and wrong, in the log.

**OPERATIONAL CONSEQUENCE for the d-4 parcel: start the server on `/run/user/1000/oracle.sock`.** That
is what every lane resolves to. Unlinking the stale `/tmp/oracle.sock` (aurora's suggestion) is **not
required** for any consumer using the reference client, since it is unreachable; it may still matter
for a client with its own resolution, which is aurora's to determine and not mine to touch.

*(The shim-half measurement is in the log. The hazard it leaves behind is live and is this:)*

**THE HAZARD TO ENFORCE, and it is this seat's job:** once the new server is the only one reachable,
every failure presents as *the consumer* being broken and the gradient pushes lanes to engineer
around gaps instead of reporting them — **bar 9's corollary with the causation hidden.** Counter-
measure: **an unserved method must fail LOUDLY and BY NAME, never degrade to a plausible answer.**
Ours already does (`-32601` unknown method; `-32602` naming the key at the dispatch choke *before*
the handler); the legacy server silently defaults unknown params, which is exactly why bar 15 says
sequence a cutover onto the STRICT implementation. **A missing capability that returns something is
far worse here than one that refuses.**

## ⚑ SIGIL CYCLE DUMPER — DORMANT (2026-08-24; detail in `OVERSEER-LOG.md`)

`blocked` on `no filed ask exists`, and sigil's own consumer (Spec 2 cycle budgets) is deferred at their
spec freeze, so neither lane has one. **Four things not to lose, each argued in the log:**
1. **Do not re-raise the join objection.** I priced this as blocked on who supplies the opcode-to-key join;
   sigil owns both halves and the gap is one adapter inside their repo. The premise was right, the
   conclusion over-reached.
2. **Branch outcome per execution, not just a cycle count** — `CycleCost::Branch` is outcome-keyed, so a
   measured count is uncomparable unless the dump says which way the branch went. A real design change.
3. **The assertion comes from the DATA:** rows carry `exact: bool`, so the gate is `measured <= modeled` on
   inexact rows and `==` on exact ones — read off the flag rather than chosen by us.
4. **⚑ The coverage number is a PREDICTION, not a measurement.** `CycleCost::Unmodeled` exists, so the
   differential's domain is partial by construction, and the deciding number — what fraction of a real ROM's
   stream is modelled — is **unmeasured and theirs to measure, before either lane spends a parcel.** Never
   let a later session cite the three-bucket split as a finding about any ROM.

## ▶ LAYER-MASK — LANDED. One safety property survives it and must not be "finished".

**`render_scanline` — the one render that commits sprite-overflow/collision latches and the R10 carry —
takes no mask and has no masked twin, so "a display mask cannot perturb emulation" is enforced by the
type system. Do not add a mask parameter to it.** Design calls and the resume path are in the log;
`docs/2026-08-26-layer-mask.md` is the artifact of record.

## ▶ QUEUED — GUI-LAYERS: the player window's layer toggles + click-an-object

**Recommended to the owner 2026-08-26, not yet picked.** The bus can now hide layers; `oracle-frontend`
cannot. It draws its own window and `pick.rs` resolves attribution **unmasked**, so a bus-set mask
changes the bus's answers and not the picture — and `pick.rs`'s *"this panel and
`emulator/pixel_attribution` must never disagree"* invariant is now **conditional on no mask being
set.** This is also the natural home for the owner's unqueued *click an object and be told what it is*.

**⚑ CONSUMER DESIGN INPUT, solicited from aurora BEFORE shaping the parcel and adopted — their

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 729-738.)*
priority order, kept.** *(Asked for deliberately: they are the editor and the only lane that would
consume this. Cheaper before the design than after.)*
1. **The answer must name its own subject, in a sentence.** Their expensive lesson the same day: they
   shipped a band lens that highlighted 1,244 cells, entirely correct, and the owner's reaction was
   *"what are the purple boxes"* — not *that's wrong* but ***what is that***. A feature that works
   perfectly and communicates nothing. So the top line is prose a person reads; the verdict enum stays
   underneath for tools. `[planeB:won, backdrop:lostToPriority]` is the right data and the wrong answer.
2. **Return identity a client can JOIN ON. ⚑ ALREADY SATISFIED — do not build it.** They asked for the
   nametable word at the dot; `pixel_attribution.cell` already returns it decoded (`tile`, `tileAddr`,
   `palette`, `hflip`, `vflip`, `priority`), iff the winner is planeA/planeB/window. **Verified live at
   `d285ecb`**, not read off the schema: `(160,100) → tile 1066, tileAddr 0x8540, palette 2, vflip`.
   **`tile` is VRAM-ABSOLUTE** (`tileAddr == tile*32`, checked: 1066×32 = 0x8540), so aurora rebases by
   `BG_TILE_BASE_SLOT` for blob-local — their model's space, and the direction their injector already
   goes. Their warning is the durable part: **an index whose space is unstated is a transpose bug
   waiting to happen**; the fragment states the space in the field's own description, which is where it
   belongs. Filed as *satisfied*, not *genuinely-new* — the triage exists to catch exactly this.
   **⚠ AND THE HAZARD ON THE OTHER SIDE OF THAT JOIN, found by aurora against this seat's own sample
   dots — if OUR panel ever names a blob slot, it inherits this check.** The rebase can land **outside
   the blob**, and `BG_TILE_BASE_SLOT = 1024` (verified firsthand as a literal at aurora
   `origin/master`), so **any `tile < 1024` rebases NEGATIVE.** Worked on the two dots this lane
   sampled: `1066 → 42`, inside their 320-tile blob; **`1456 → 432`, outside it** — and *not rescued by
   capacity*, since 432 < 448. **Their durable formulation: in-capacity is not in-blob.** *(Base slot is
   ours firsthand; the 448 capacity and the 320-tile blob are their measurements, not re-derived here.)*
   Plane B can legitimately be showing engine art, another act's art, or a slot past the blob's end —
   the corpus-ROM sample is not a defect, it is proof the class is reachable. **So a click-to-identify
   surface must answer *"that is not part of your background"* for those — not index, and above all not
   guess: an unchecked rebase either throws or confidently names a slot the author does not own, which
   is indistinguishable from a correct answer.** That is point 3's loud-on-unmeasurable rule arriving on
   the join instead of on the mask, which is what makes it a class rather than two tips.
3. **Assert the conditional invariant rather than noting it.** A rule with an unasserted precondition is
   this workspace's recurring defect; their form is **loud-on-unmeasurable beats a plausible answer** —
   their layout harness answers *"COULD NOT MEASURE A FIT"* under a planted defect rather than "fits".
   So while a mask is set the panel SAYS so in the answer rather than quietly describing a picture that
   is not on screen, and the human-facing line carries it, not only the wire caveat.
4. They will honour the framebuffer-digest ruling and not fingerprint a masked view.
5. **A lens must state that it is on, persistently, and this is a CORRECTNESS requirement rather than
   polish** — their point, and it had not been on this seat's list. A mask that changes the picture with
   no standing on-screen statement is the unlabelled-highlight defect one level up: *the author will
   forget, and then read a masked picture as the real one.* Their canvas palette treats colour as a
   language deliberately; a toggle that fights that is worse than none.


## ⚑ HUB RULING, 2026-09-02 — HERMETIC GATE IS THE RATIFIED SHAPE; DRIFT IS A NIGHTLY, AND IT GETS **NO SECOND OWNER CARD**

⚑ **RELAYED BY empyrean-01, NOT WITNESSED BY THIS LANE** — same flag, same reason, as the relayed
rulings above. It is the **hub's** ruling under the owner's standing delegation. Do not upgrade it to
his. Anchored at empyrean **`1e9d70c`**, verified firsthand here rather than taken on trust: the object
is a **commit**, it is an **ancestor of their `origin/main`**, and `--stat` shows it is a **docs commit
carrying a docs ruling** — so its SHA class matches what it anchors.

**The question this answers** is the one `parcel/stopprecision` left open in
`docs/2026-09-02-stopprecision.md` §6: our schema gate went hermetic (blob-pinned, no peer read), and
the deliberate cost recorded there was that **a default run no longer notices upstream moving on its
own**. **Ruled: the hermetic default is the ratified shape, and drift detection is a NIGHTLY's
property, never a local run's.** Same shape as sigil's decouple — vendored content plus a revision
stamp, local runs hermetic, drift watched out-of-band.

**THE OPERATIVE INSTRUCTION, and it is a prohibition — read it before filing anything:** the drift job
is **a queue row here, not an owner card.** The host question it would raise (a standing unattended
timer on the owner's machine) is **already open with him as empyrean `d-9`** — verified firsthand at
their `origin/main`, and its question is literally *"Running it means a systemd timer on YOUR machine
… Do you want that standing job installed?"*, which is the same question ours would ask in different
words. **One cross-lane question gets one card.** A second card does not add information; it makes him
answer the same thing twice and lets the two answers diverge. *(`d-7-restated-3` is the companion card
— how many quiet chains before review — provisionally ruled N=5.)*

**Board row id: `SCHEMA-DRIFT-NIGHTLY`** — this section is that row's detail, per `LANE_STATUS.md`
rule 7 (a title states the state; the history lives here and the row points at it by id).

**The shape to build, when it is picked up:** a runner with `AETHER_CONTRACT_REPO` set, **non-blocking**,
reporting *"contract advanced past pinned blob"*. Note it needs **no new capability** — the hermetic
gate already grew exactly that env-var path as step 2 (`schema_conformance.rs`), so the nightly is a
caller of a road already built, not a build.

**Also carried in the same message, both banked:** our landing is recorded upstream with attribution
(**2026 passed / 0 failed / 6 ignored** cited as *our* measurement, not re-derived by them — correct
attribution discipline); and **F-RESUME-STOP-RACE was relayed to aurora** as the suite's outbound
client, which is the right destination — that register entry names `tests/breakpoints.rs` and
`tests/watchpoints.rs` as still carrying the racy spelling, and aurora writes clients that will hit the
same read-through-discarding-events shape. **No reply was requested and none is owed.**

⚠ **The one thing verified against MY OWN interest, because the relay asserted it and this seat's bar
says a claim about your own tree gets read out of the file:** the blob identity is **content-addressed
and therefore not talkable-into-agreeing** — our vendored
`crates/oracle-aether/tests/contract/bus-protocol.schema.json` is `125d17f03ac33872…` at our `HEAD`,
and `git rev-parse 82982b7:contract/schema/bus-protocol.schema.json` in empyrean returns **the same
blob id**. Byte identity by construction, checked in both trees, neither read from a working file.

**Board row id: `ATTR-RGB-LATCH`** — detail lives in `docs/2026-08-30-rgb-live-resolve.md`
(aeon's colour finding: reproduced 55/55, closed as a server change; ~~what remains is a contract change
so the reply says which moment its colour is for and names `emulator/scanlines` as the caller's path~~).
Anchored here 2026-09-02 because the row's own title carried the only copy, and `LANE_STATUS.md` rule 7
requires the row to point at its detail by id.

⚠ **CORRECTED 2026-09-03 — THE CR IS FILED AND ADOPTED; WHAT IS OWED IS OURS TO BUILD.** The struck sentence
said a CR still had to be filed. **CR-G was ours, and was adjudicated `ADOPT WITH CHANGES` as
`contract/protocol.md` §11.27** at empyrean **`32a0041`** (2026-08-30T02:37Z; ancestor-verified, `--stat`
shows protocol +48 and schema +6, so the SHA class carries what it anchors). *(How the row stayed wrong for
four days, and why the hub was right to change our emission rule: `OVERSEER-LOG.md`, 2026-09-03.)*

**What §11.27 leaves owed, read out of the adopted text and not summarised from memory:**
1. **The emission rule is a MEASUREMENT, and it is NOT the one we proposed.** Adopted: emit when the CRAM
   entry at `cramIndex` **has been written since line `y` of the last completed frame was drawn**, or when
   no frame has completed; absent otherwise. A server that cannot yet stamp per-entry writes **MAY** emit on
   *any* CRAM write since the line drew — coarser, still conditional — and **MUST NOT emit unconditionally.**
2. **Four vectors, and §11.27 names US as their author**, red-first, run against the schema before handover
   (the bar this lane set itself on CR-F): caveat after a qualifying write (valid); a caveat naming no method
   (red); a pre-first-frame reply carrying it (valid); no qualifying write and no caveat (valid).
3. **The "required when applicable" half is a LIVE CONFORMANCE CHECK, never a schema property** — a schema
   cannot see the write stamp. It rides with our conformance rows the way `object_at`'s did.

**State measured here 2026-09-03, both sides.** The vendored fragment at our pin **already declares**
`caveat` on `emulator/pixel_attribution`'s result, with §11.27's rule quoted in its own `description`; and
`Engine::pixel_attribution` **never sets the key**. So we are conformant-by-omission (the fragment does not
require it) and silent on exactly the divergence aeon asked us to make audible. **F-SCANLINE-INDEX is
untouched**: §11.27 makes the divergence audible, it does not close it.

⚑ **AND THE SCOPE OF WHAT THE HUB CAN HAND US, ESTABLISHED 2026-09-02 BY THE HUB RETRACTING ITS OWN
GO — bank this, it will recur.** At 10:40Z the hub cleared `LIVE-TREE-RESIDUE` "under the owner's
widened delegation". **This seat held anyway** and the hub then **withdrew it against its own
interest**, which is the strongest form this correction could take.

**The test that decided it, aeon's, and it is the reusable part: *is there an owner decision under
this, and is it THIS question?*** Applied here: the owner's 03:22Z words (empyrean `63c85ae`) re-arm
the **raster/parallax effects** drive; his 03:46Z widening (empyrean `4e8e865b`) covers **decision
CARDS in a lane's domain**. Neither is a word to start a **non-effects** parcel, and the live-folder
cleanup is this lane's own hygiene row. The nearest owner decision under it is `d-17` (his — the
*write* side into his aeon folder); the *read* side, sigil's `d-18`, **was ruled by the hub under
delegation, not by him.** So no owner decision exists for this question, which is exactly what the
test asks.

**THE DURABLE SPLIT, and it is what a future session should apply without re-deriving:** under today's
brief the hub can hand this lane **any ask an effects lane files** — those need no owner word and
should just be worked. It **cannot** hand us a go on our own hygiene, our own backlog, or anything
outside the effects drive. **When a relay's go and this test disagree, the test wins and you ask the
owner.** Precedent from the same morning, banked in the hub's own record at empyrean `13a7d5a`: sigil
adopted a hub *ruling* while holding for the owner's *word* on the same shape — **a relay carries a
ruling, never an authorization.** Two lanes, same hour, same conclusion, reached independently.

## ⚑ `run_to` vs `resume` — TWO LIVE RULES (2026-09-02; derivation in `OVERSEER-LOG.md`)

Answered for aurora and re-derived by them independently at `7ba2faf`; the exchange is closed and its
mechanism reading is in the log. What stays live:

1. **⚑ ON THE WIRE, `run_to`'s `stopped` EVENT PRECEDES ITS REPLY** — it calls `emit_stopped` before it
   builds the result. A client that reads through to the reply and discards events (aurora's, and
   `Client::ok`'s) is correct and unaffected. **A client that consumed the reply and THEN waited for the
   halt event would block forever** — F-RESUME-STOP-RACE with the halves swapped, and exactly the shape a
   first breakpoint consumer reaches for. `run_to` blocks inside `dispatch`, so its reply is *produced by*
   the halt; `resume` only flips a flag and returns, which is the whole of that race.
2. **`"reached": run.predicate_fired` — the predicate's own verdict, NEVER the sink's — now has a named
   live consumer** (aurora's boot restore gates on `reached !== true`). `StopRecord::fired` means only
   *"something asked to stop"*, so reading it would report a target as reached because an unrelated
   `stopAfter` watch halted the run. The "simplification" that swaps them would break a real client
   **silently, in the direction that presents as a successful boot restore over a write window that never
   opened.** Booked here because a code comment is where a perishable rule goes to be read by nobody.

## ⚑ OWNER RULING, 2026-09-03 — WHAT GETS A TAB IN THE DEBUG WINDOW (ORACLE-DEBUG-UI)

**Witnessed directly in session, not relayed.** Put to him as an assessment, answered *"That's fine I agree
with the assessment"*. It is the standing shape for every panel parcel; do not re-derive it.

**Default: a capability served on the bus is reachable in the window, not only from a tool.** That is his
standing "build out the tooling" directive arriving on the UI. But **a tab is not the right shape for all
of them**, and the split is the ruling:

* **Things you LOOK AT** — registers, memory, objects, breakpoints, profiler — **are tabs.**
* **Things you DO** — reset, press, spawn, write — **are NOT tabs.** They are controls inside a panel or an
  invoked command. A tab that is empty until used is a worse button. *(The spawn serve is the live example:
  its surface is clicking a spot in the Screen panel, not an `object_spawn` tab.)*
* **Things too expensive to show live** — `scanlines` is ~440 KB of JSON per frame — are **on demand**,
  never a docked tab quietly costing frames.

⚑ **And the half that is a correctness rule rather than taste: A PANEL MUST SHOW THE SAME ANSWER A TOOL
GETS.** ~~Prefer reading through the served surface over reaching into the emulator by a private route.~~

⚠ **THE REQUIREMENT IS HIS AND STANDS; THE MECHANISM WAS THIS SEAT'S AND IS WRONG — corrected 2026-09-03,
original struck rather than deleted.** He agreed to an assessment that contained my error, so the record has
to separate them. **"Route panels through the served surface" is the option this repo already considered and
REJECTED, with the reasoning in-tree the whole time:** **the primary source is the contract itself**, `empyrean:contract/protocol.md:238` (D15), not our
comment about it: *"An in-process GUI is a consumer of the same registry, not a second server. A debugger
or inspector view living in the player's own window **reads the method registry directly, in-process; it
does not open a socket to itself.** The one legitimate alternative — a GUI running out-of-process as an
ordinary Aether client…"*. So the contract does not merely reject the round-trip, it **prescribes** the
in-process read and names the only alternative. `pick.rs:649-655` is our in-tree echo of it, and our
`Host::pump` makes the rejected shape worse still — a click would enqueue and wait a frame to answer what
it can answer synchronously. *(Re-anchored 2026-09-03: this correction first cited the code comment, which
is the story rather than the artifact — this repo's own bar.)*
**What makes parity true by construction is ONE IMPLEMENTATION UNDER TWO CONSUMERS plus a parity test**, not
a transport. `host.rs:439-440` already says so: panels draw from *"the same instruments its loop feeds and
the bus serves, so a local readout and a client's reply cannot disagree."*
**The line, from the parcel-2 design:** per-frame panel bodies read the shared derivation **directly**;
per-gesture **commands** go through a synchronous `Host::call`, so a click gets the tool's exact reply *and
its refusal*. That grants his correctness half in full and costs JSON per click, not per frame.
**Mirror he did not state and it follows from his own default:** a served capability that **changes what the
window does** must be *visible* in it — `hold` ORs a client's pad into the player's, so a disconnected client
can leave someone walking left forever with nothing on screen able to say why.

**Parcel-2 line items this settles:** layout persistence is one `serde` flag and is deliberately OFF until
the placeholders are gone (saving a layout of placeholders buys a migration); `Tab::Registers` content is
filler on purpose while the docking it exercises is real.

## The bars (house methods — each earned by a measured failure; do not thin)

**▶ PARKED, NOT IN FORCE (moratorium above; parked at the hub in `OVERSEER-PENDING-BARS.md`) — A PARITY
PAIR IS STRUCTURALLY BLIND TO A DEFECT IN THE DERIVATION IT SHARES. ASSERT THE SHARED DERIVATION DID
SOMETHING.** Found by this seat probing parcel 2b, where the defence
already existed and is the reason the probe is a bar rather than a bug. R1 ("one derivation, two
consumers") makes a panel and a handler agree **by construction** — which is the point, and which means a
parity test can only witness *agreement*, never *correctness*. Break the shared function and both sides
move together: the pair agrees perfectly and both are wrong. Measured: `absolutise` reduced to
`path.to_string()` leaves the strip and `emulator/status.romPath` in exact agreement on the un-normalised
string. **The remedy is a third assertion in the pair — that the derivation is not a no-op** — and 2b's
test carries it (`assert_ne!` against the raw argument, failing with *"the agreement above is two copies
of the same untouched string rather than one shared normalisation"*), so the mutation went red. **Every
R1 pair owes this third clause**; without it a parity suite grows more confident exactly as it shares
more code. Same family as the poison bars: the row measures a real quantity and not the one it is named
for.

**▶ NEW BAR, 2026-08-26 — A MERGED SERVE IS NOT A SERVED METHOD. THE CONSUMER REACHES A BINARY.**
Found in the foreground pass that closed the CR-D `⟨RUNTIME⟩` debt
(`docs/2026-08-26-runtime-decoders-check.md` §5). The object decoders merged, tested and pushed at
`0f33c44` — and stayed **unreachable to every consumer**, because `target/release/oracle-aether` was
still the build from the day before and **nothing in a merge rebuilds it**. The MCP shim spawns *that
binary*; so a shim spawned any time between the merge and the check answered
`[-32601] no such method` to the very methods we had just shipped. Reproduced firsthand on this
session's own shim, then fixed and re-verified end to end through the consumer's own spawn path.
**This sharpens, and does not contradict, the coordination note that *advertising a method is
shipping it*: the advertised list is authoritative, but it is emitted BY A RUNNING BINARY, and a
stale binary advertises a stale list with total confidence.** Practical check before telling any
consumer a method is available: spawn the consumer's own path and call it — not `cargo test`, which
passes against source the consumer never runs. Same family as item 1's rename fallout
(*compile-time-frozen paths, invisible until the binary runs*); here the frozen artifact was the
binary itself.

**▶ AND THE COUNTING BAR THAT CAME WITH IT — MEASURE USE, NOT ATTACHMENT.** The same pass had to
count whether consumers actually call these methods. `grep -c` over the transcript tree reports
~10,000 mentions across ~4,055 files — and reports **the same ~4,055 for every tool name, including
tools nobody has ever called**, because the MCP tool listing sits in every session's system prompt.
Parsing `tool_use` blocks instead gives the true figure: 216 invocations. **The near-constant across
varied inputs was the tell** — the existing bar caught it. Mentions measure attachment; only
invocations measure use.

**▶ NEW BAR, 2026-08-24 — ANCHOR A CLAIM TO A SHA THAT CAN CARRY IT. A docs commit cannot vouch for
code.** Caught by aeon against this seat, same day. I reported the straddle fix to them anchored to
`7bdb75f` — which is a **one-line `docs/lane-log.jsonl` commit**. Every claim I made was true, and
the anchor could not carry any of it: the code is `4111c88` under merge `51143a5`, tests `68461a7`.
They cited the code SHAs in their booking instead. **The failure mode is that it hardens invisibly** —
a peer transcribes the anchor into their prose, and a later reader who checks it finds a docs diff
where a guarantee was promised. This is the same family as the provenance audit above (*cite the
ruling, not a status field*): the citation must be the artifact that actually contains the thing.
Practical check before sending: `git show --stat <sha>` and confirm the files named are the ones the
claim is about.

**▶ AMENDMENT, 2026-08-27 — THE ABOVE BAR HAS A FALSE-POSITIVE MODE, AND THIS SEAT FIRED IT AT A PEER.

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 952-966.)*
A RIGHT SHA ANSWERING AN UNSTATED QUESTION IS NOT A WRONG SHA.** Found by aiming the 08-24 bar at aeon
and being half right; the diagnosis below is theirs, banked by them at aeon `b64f6bcb` (verified here as
a reachable ancestor of their `origin/master`, docs SHA carrying docs).


**⚑ AND THE HALF THAT COST ME MORE THAN THE CATCH: RUN `--stat` ON THE SHA YOU PROPOSE, NOT ONLY ON THE
ONE YOU DOUBT.** This seat named a replacement anchor by inferring from a commit's subject line and it was
also a docs commit; on the one chain where it was measured, subject-line inference failed at two in three.
A subject line describes what a commit is *about*; `--stat` is the only thing that says what it *contains*.
*(The archaeology: `OVERSEER-LOG.md`, 2026-08-27.)*

**⚑ THE PROCESS LESSON, which aeon called out explicitly and which is why this was cheap: I sent it

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 987-989.)*
HEDGED — *"treat this as a reading, not a finding"* — and that is what made it worth sending.** It was
50% right (symptom yes, diagnosis and replacement no). Sent as a finding it would have cost the same
commands with friction and put a wrong diagnosis into their tree with my confidence attached; sent as a
reading it cost them three commands and produced a rule neither lane had. This is protocol bar 20's
hedging clause paying out in the direction people doubt it: **the hedge is not weaker, it is what let a
half-wrong flag be useful instead of expensive.**


**▶ AND THE SCOPE-MARKING BAR IT ARRIVED WITH, which is aeon's and is the more reusable half.** Their
mis-filed ask traced back to a sentence **in our own module docs** — *"`self_cycles` has no such
lag"* — that is true of routine rows and false of interrupt buckets and **did not mark which it
meant**. They carried it across the boundary; the sentence let them. Their framing, worth keeping
verbatim: *"a relayed premise inherits no more scrutiny than the claim it supports."* **A rule that
is true of one kind and silently false of another must say which at the point it is stated**, not in
a later paragraph a reader may never reach. Fixed at source in `profiler.rs`. Also theirs, and
sharp: they sorted the gap **from our wire schema** (no such key, therefore genuinely-new) rather
than **from the quantity they needed measured** — and a schema can only tell you whether a *name*
exists, so sorting from it lands in the expensive bucket by construction.

**▶ NEW BAR, 2026-08-24 — `docs/lane-status.json` is the OVERSEER'S file. Never let a dispatched
agent edit it, and say so in the brief.** Earned the same day: the Q-PROF-STRADDLE agent did
excellent work and, closing out, marked its queue item `"state": "done"` — an enum the suite
contract does not define. The Dominion console **rejects the whole document on one bad enum**, so
that single word would have made this lane invisible on the owner's board for the second time in
one night. The agent could not have known: the valid states live in `empyrean/contract/LANE_STATUS.md`,
not in this repo, and nothing it could read locally would have told it. **The fix is structural, not
educational** — a live operational file that the console parses is not part of any work product, and
handing it to an agent puts a contract the agent cannot see in the path of a commit it must make.
Agents report their queue outcome *in their report*; the overseer transcribes it. Related: a
finished item **leaves** the queue — `done` is not a state, it is an absence.

> ⚠ **READ THE PROTOCOL AT A COMMITTED REVISION, NOT THROUGH THE PATH** (seraph's rule, empyrean
> `origin/main` — the most upstream rule in that document): `../empyrean/docs/OVERSEER-PROTOCOL.md`
> is **one peer's live working tree**, so booting by path delivers the suite's shared contract by
> reading somebody's uncommitted directory. Use
> `git -C ../empyrean fetch -q origin && git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md`.
> Correct citation discipline applied to a bad source produces a **more** convincing artifact, not a
> less convincing one. **This session's own boot is the measured case**: it read the file by path
> while empyrean held sixteen unpushed commits, and got the right bytes only because their worktree
> happened to be clean at that minute — 59 lines landed in that path minutes later, and the file
> reached **422 lines by day's end against the 245-line snapshot the session booted on**. Right
> answer, by timing luck, with nothing in the output saying so.
>
> The commentary on protocol bars 8-15 moved to the log: they are read at boot from the protocol
> itself, at a committed revision, which is the only copy that cannot drift. The stanza above stays
> because the protocol's own bootstrap exception sanctions it.


- **Contract-first, always**: CR → un-framed adjudication → apply fixes → the code and its
  amendment merge in one window so `protocol.md` never describes a server that does not exist.
  Post-adjudication changes ride **deltas** (same adjudicator, same standard). Adjudication is not
  optional even for your own rulings — a ruling authorizes the change; adjudication is what
  authorizes the *text*.
- **Verify firsthand before accepting**: run fmt + clippy ×2 + the full aggregate yourself
  (`cargo test --workspace 2>&1 | grep -E "^test result" | awk '{p+=$4; f+=$6; i+=$8; n+=1} END
  {print "LEGS="n" PASSED="p" FAILED="f" IGNORED="i}'`). Agent reports have matched every time —
  verify anyway; the one time they don't is the point.
- **Serialized cargo**: NEVER two cargo runs anywhere in this repo at once — including
  isolated-worktree runs while any agent runs cargo (measured: legs truncate with a spurious
  failure, three data points). Queue acceptance gates; verify BEFORE resuming an implementer.
  Short release builds for an owner-facing unblock are the recorded exception (nice -19, logged).
- **⚑ RED-FIRST IS NECESSARY AND NOT SUFFICIENT — a poison can come back GREEN with the guard
  perfectly sound** *(2026-08-22, aurora; three green poisons in one parcel, none of them a bad
  guard)*. The three classes: (1) the row aimed at a branch **a pre-check makes unreachable**;
  (2) the row proving only *"it refused"*, which **two independent code paths** both satisfy — so
  deleting the guard under test leaves the *other* mechanism holding it green (the matcher clause,
  but the collision is two **paths** producing one observable, not two messages sharing a phrase);
  (3) **the row measuring the WRONG OBSERVABLE** — the fixture left the thing resolvable, so the
  catch site the test was *named after* was never entered. **Planting a violation could not have
  revealed the third; only asking whether it measured the right quantity could.**
  **⚑ THE TEST FOR WHETHER A SPLIT LIKE THIS IS REAL — aurora's, and it generalises past poisons:
  do the two classes have DIFFERENT FIXES?** A matcher collision is repaired by re-pointing at wording
  only that rule uses. Two-paths-one-observable **is not repaired by touching the matcher at all** —
  the matcher can be perfectly precise and the row still worthless; it is repaired by asserting
  **which path ran**. *A bar that cannot tell them apart sends you to the wrong repair*, which is the
  cost of collapsing them. **Their tell for the confusable pair: is the observable UNIQUE to the
  rule?** Unique → the assertion is too loose (matcher). Not unique → the assertion may be exact and
  still prove nothing (two paths).
  **Operational form, to be asked per assertion:** *if this row went green for a reason OTHER than
  the rule holding, what would that reason be?* — then check that specific reason, and report the
  alternative green-path considered and how it was ruled out. **A `None`/absent/empty on either side
  of a comparison must be LOUD, never green**; that is where all three hid, each reading as healthy.
- **Mutation discipline**: every evidence-bearing test carries a recorded mutation (edit → touch →
  observe "Compiling" → named FAIL → revert → green; cargo's fingerprint is MTIME-based). A
  mutation that catches nothing is strengthened BEFORE recording, never recorded hollow. When an
  expectation and the code disagree, investigate to ground truth — three times today the code was
  right and the expectation wrong.
- **Currency scrutiny**: goldens never regenerate silently; every mover carries a named, measured
  mechanism in its `cause:` comment; any unexplained mover is a STOP-and-report, not a re-pin.
  Zero-file-diff on `crates/oracle-core/tests/` is the default expectation for bus work; breaking
  it is a named decision.
- **Demands are committed artifacts**: transcribed from the consumer's own source with anchors
  (never from a relay — relays get flagged as such until an anchor lands), corrections recorded
  supersession-style (original visible, correction over it), gap triage into
  satisfied / composable-today / genuinely-new.
- **⚑ A CONFIDENT MECHANISM FROM THIS SEAT IS A HYPOTHESIS, AND THE RECEIVER'S OWN ALREADY-RUN
  COMMAND OUTRANKS IT** *(2026-08-22, found by the sigil lane against themselves; proposed upstream
  to empyrean, which is where it belongs — do not treat this entry as the rule's home)*. I sent a
  peer a confident mechanism for why three stale citations survived (*"they resolve into a different
  real repo and hand you a plausible wrong file"*). It was **wrong** — the leaves 404. My error was
  reusing a real lesson on an instance I had not measured. **Theirs was worse and is the durable
  half: they had ALREADY RUN the refuting command in the same session** — a directory listing and a
  probe that printed `No such file or directory` — **read the output, used it to conclude the cite
  was stale, and then wrote a row asserting my mechanism anyway**, because mine was a better-sounding
  story and arrived with a post-mortem attached.
  **Why this is a bar and not an anecdote: the second failure does not need a peer to be wrong — it
  only needs a peer to supply the frame.** A confident mechanism overwrites a measurement the
  receiver already holds, silently, and nothing in either session looks like a conflict because the
  measurement was never re-read.
  **▶ THE DELEGATION COROLLARY, which is the operative half for this seat and is strictly worse than
  the peer case.** A peer has standing to push back; **an agent has almost none.** Four of my stated
  facts were corrected today by agents who checked them — and every one of those was a *fact*, which
  is checkable. **A stated MECHANISM is far more dangerous than a stated fact**, because it explains
  the evidence rather than competing with it: an agent that measures something inconsistent with my
  mechanism will tend to reconcile the measurement *to* the story instead of reporting the conflict.
  **So: state mechanisms as hypotheses in briefs, explicitly labelled, and say in every dispatch that
  the agent's own command output outranks anything I asserted.** The instance that saved us here is
  the shape to demand — the README agent verified all three targets **individually before writing**,
  rather than performing the search-and-replace my framing implied.
  **Second clause, sigil's, and it prevents the over-correction:** when a correction lands, **check
  which half of the claim actually moved before discarding the whole thing.** The rename shape and
  the code-comment rule both survived my wrong mechanism intact, and retracting them along with it
  would have destroyed two sound rules to fix one bad sentence.
- **Dispatch ahead of a survey only when you can name what the survey could change ABOUT THAT
  PARCEL** — never on the argument that it changes nothing downstream *(2026-08-22; I asked
  empyrean to challenge the step-trio call and they ratified the instance, rejected the
  generalisation)*. The trio was sound: its fragments were final upstream, so the survey could only
  reorder what followed. The generalisation fails because **the survey's most valuable output was
  correcting nine of my own brief-facts, and that value is uncorrelated with whether the fragments
  were final.** Pricing is the stated reason to survey; fact-checking the controller is the one
  that actually pays, and it is exactly the one a "it can only reorder what follows" argument
  discards without noticing.
- **Never record an approval whose granting act you have not seen — cite the ruling, not a status
  field** *(2026-08-22, from empyrean, who found it in their own doc; see the Fable-seat audit in
  The role above, which is this lane's instance)*. Boot docs are snapshots that age while logs
  accumulate, and an owner ruling lands in the middle where head-and-tail reading never sees it —
  so **grep the history for an item before putting it to the owner OR funding work off it.** Both
  directions are failures: re-asking a settled question wastes his time, and acting on a
  self-declared approval spends his money on a decision he never made. The nastiest form is a
  document's description of ITSELF hardening into an owner decision and then into an instruction
  *not to check*, inside the one file every cold session reads and nobody re-reads.
- **Dedicated adversarial review** for load-bearing slices (the slice that carries an arc's central
  claim gets its own reviewer with explicit targets and required explicit negatives).
- **Better-than-the-floor** on every request; improvements additive so the migrating consumer
  loses nothing; the pre-release window for REQUIRED additions shuts at first ship — spend it
  deliberately, once.
- **A gate described for someone else to carry must name its ASSERTION, not its shape.** Earned
  2026-08-23 with aeon, on both sides in one exchange. Our reconciliation identity is a **loss**
  detector, not a correctness proof: a suppressed interrupt bucket *conserves* its cycles into
  `unattributedCycles`, so **the identity closes with that term arbitrarily large** and closure alone
  is satisfied by exactly the case the gate exists to catch — only the explicit `== 0` assertion
  fires. A peer booked the requirement as *"carries the identity check"*, having read the proof, and
  would have shipped a **correctly-described gate whose teeth were gone**: not a wrong gate, not a
  missing one, a gate whose shape a porter inherits with no reason to look under it. Note where this
  bit: inside the very booking written to argue that a mechanism beats a remembered rule. **The
  mechanism only beats the rule if the assertion survives transcription** — so when writing a gate
  into prose for a consumer, name the thing that fails, and re-derive rather than paraphrase when
  carrying someone else's.

**▶ NEW BAR, 2026-08-26 — CHECK THE VINTAGE OF THE PROCESS, NOT THE VERSION OF THE FILE. A long-lived
interpreter is a stale artifact class, and no in-tree check can see it.** Found by this seat while
reaching for an unrelated ⟨RUNTIME⟩ debt; corroborated independently by aurora, sigil, seraph and
dominion within the hour. `oracle-old` `07314aa` (08-25 21:09) made the MCP shim spawn its own private
`oracle-aether` and stop dialling the well-known socket. **Every suite lane's shim process started
08-25 19:53–20:29 — before that commit — and Python reads its source at process start.** So all six
lanes were executing the pre-ruling version, wired straight into `/run/user/1000/oracle.sock`, held by
the OWNER'S live `oracle-frontend` player. Proven by socket-inode pairing for oracle and aeon; aeon had
already `reload_rom`'d his running window onto a worktree build at ~18:20Z in perfect good faith,
**on a banked note that said a fresh session gets a private instance** — a note that was true of the
file and false of the process.
**This is yesterday's *a merged serve is not a served method* bar on a NEW artifact class.** That one
names compiled binaries and compile-time-frozen paths. This is neither: the file on disk is correct,
the fix is merged AND pushed, `git log` looks finished, and the defect exists only in the memory of a
running process. **No sweep, no audit, no cold read of the tree can reach it** — the tree is right.
**⚑ AND THE REMEDY IS NOT THE OBVIOUS ONE: a `/clear` does NOT fix it; only a session relaunch does.**
The shim is spawned by the session process, so clearing the conversation leaves the same interpreter
running. Measured firsthand here, and this is the cheap corroboration worth copying: **this session was
`/clear`ed and its shim's start time did not move** (shim 287372 at 20:29:19, one second after its own
session process at 20:29:18). aurora reached the same conclusion from the *other* direction — that the
shim is on the process command line — which is bar 19's genuine corroboration rather than echo, because
neither derivation could have shared the other's parameter.
**aurora's one-command discriminator, adopted: `pgrep -P <shim-pid>`.** A post-fix shim owns a child
`oracle-aether` on a `/tmp/oracle-mcp-*` mkdtemp socket; a pre-fix one has no child. Both kinds appear
in a single `ss -lxp`/`pgrep` listing, so pre- and post-fix sessions are **visibly different in one
command** with nothing to reason about.
**The failure that nearly happened to three separate lanes' documentation, and it is the durable half:
aurora's own `OVERSEER.md` asserted the opposite** — *"`mcp__oracle__*` in this session SPAWNS A PRIVATE
EMULATOR by default — it is NOT the window the owner is watching"* — written that same day, correctly,
**from the file on disk**, and false for every interpreter older than 21:09. They fixed it at
`83fcb64`. **A claim about RUNNING STATE banked as though it were a property of the code** is the
perishability preamble's sharpest instance yet: the anchor was valid, the source was authoritative, and
the sentence was still wrong the moment it was written.
**Operational form: before trusting any tool that dials something, ask when its PROCESS started
relative to the fix you are relying on** — and write the vintage condition into the note, never the
conclusion alone.


**▶ NEW BAR, 2026-08-27 — THE OPS LINE THAT IS NOT IN THE DISPATCH IS NOT IN THE DISPATCH. Carry the
worktree `vendor` symlink into every brief that will run cargo.** The Ops section below has said *"fresh
worktrees: `ln -s <repo>/vendor vendor`"* for weeks. It was **still missed on a dispatch this morning**,
because the brief is composed from the invariant block and the parcel's own grounding — and an Ops line
sitting in this file is not either of those. The agent lost time on a baseline that would not reproduce:
eight `save_state::tests::*` rows **panic** (not skip) on the missing vendored ROM, and the resulting
`exit 101` is indistinguishable at the aggregate line from two other causes this repo has recorded.
**The fix is structural, not educational** — an overseer who has read this file every session still omitted
it, so the rule is that the vendor line is part of the *brief template* for any cargo-running dispatch,
alongside the base check. Related and already booked: the same class as *a merged serve is not a served
method* — knowing a thing in the tree is not the thing reaching the process that needs it.


**▶ NEW OPS LINE, 2026-08-30 — NEVER CITE THE TIP. CITE THE COMMIT THAT CARRIES THE ARTIFACT, EMITTED

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1196-1220.)*
FROM THE PATH.** Third instance of the anchor-class family against this seat, caught by the hub.


**The corrective is constructive, not verifying**, because `--stat`-after-the-fact is what the existing
bar already prescribes and it did not fire — I had no reason to doubt a hash I had watched go out:


**▶ NEW OPS LINE, 2026-08-30 — A KILLED SUITE LEAVES A LOG THAT AGGREGATES CLEAN. COUNT THE LEGS, NOT THE

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1226-1252.)*
FAILURES.** Nearly quoted as a merge verdict by this seat.


**Corrective, and it is cheap because it is one more line in the same command:** a verification asserts
its own **completeness** before its verdict —


**▶ CORRECTION, 2026-08-30 — OUR HEADLESS RECIPE'S "BOTH GUARDS" ARE ONE GUARD TWICE, AND IT IS THE

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1259-1285.)*
GUARD A PEER JUST MEASURED AS INEFFECTIVE.** Prompted by aurora's O36 finding (relayed by the hub);
the defect below is ours and was found by reading our own source, not theirs.


**▶ NEW OPS LINE, 2026-08-30 — DO NOT COMMIT WHILE A VERIFICATION RUN IS IN FLIGHT. IT INVALIDATES THE

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1292-1312.)*
BUILD-ID GATE AND THE FAILURE LOOKS LIKE THE PARCEL'S.** Third instrumentation failure in one night, and
the only one that produced a red that was entirely mine.


**Corrective:** a verification prints `HEAD_AT_START` and `HEAD_AT_END` and **they must be equal for the
verdict to count**. Bank findings *after* the run, never during — the twelve minutes are not free time,
they are part of the measurement.


**▶ SCOPE CORRECTION, 2026-08-30 — `screen_text` DOES NOT END AEON'S EYEBALL REQUESTS, AND THIS FILE SAID

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1319-1330.)*
IT WOULD.** Corrected by aeon against a claim this seat made to them, which was taken verbatim from our
own queue row.


**The durable shape, and it is why this is an ops line rather than a typo fix: the over-claim was in a
QUEUE TITLE, which is the one place nobody re-derives.** It was written when the item was a sketch,
inherited by every status file since, and finally exported across the fence with a seat's confidence
attached — where the only reader who could refute it happened to be the party it was about. **A queue
row's justification ages exactly like a precedent narrative and nothing re-reads it.** When an item's
design lands, re-read its own queue title against what was actually built.

**▶ F-ACCEPT-TABLE-CROSSCHECK-BLIND (registered 2026-08-30, emitter behaviour change, NEEDS A RULING).**

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1351-1358.)*
`tools/legacy_accept_table.py`'s axis-A/axis-B reconciliation adds to `claimed_lines` **before the row is
written**, so it is **structurally blind to a row-level drop**. Measured firsthand: with the four unguarded
`addr` rows dropped cleanly, `--fail-on-gap` prints `cross-check : AGREES`, `parse complete : yes` and
**exits 0**, while `UNGUARDED reads` silently falls **43 → 39**. ⚑ **So the tool's own headline safety line
is not evidence of the thing a reader takes it for** — it witnesses that every *access* was claimed, never
that every *row* survived. The 57-test suite catches this by named assertion; `--fail-on-gap` does not, and
`--fail-on-gap` is what a CONSUMER would wire into a gate. **Revival: aeon wiring the table into their gate
— they must be told the gate flag is weaker than the suite** (told 2026-08-30). Fix would be `--fail-on-gap`
independently verifying row presence against a source-derived expectation; that is an emitter behaviour
change and was correctly kept out of the hardening parcel.


## Ops (each line is a paid-for lesson)

**▶ `lane-status.json` — THE BOOT CURL VALIDATES THE FILE YOU WROTE AT BOOT AND NOTHING AFTER IT** (2026-08-30,
this seat, measured). I wrote `"state": "done"` on a landed row after a merge. **`done` is not in the
vocabulary** (`doing | next | open | blocked`; a landed row LEAVES the queue and its landing goes to
`lane-log.jsonl`). **One bad enum in one row rejects the WHOLE file**, so the owner's card for this lane went
dark — with every true thing in it — and stayed dark for about an hour. **Nothing about it is visible from
this side:** the file writes fine, `git` is happy, and the lane goes on reporting accurately to itself.
**The defect was not the word, it was that I ran the verification curl ONCE, at boot.** Every later write is
unverified unless re-checked, and I made a dozen. **Re-run the boot step's curl after ANY write to
`lane-status.json`** — it is two seconds and it is the only thing that can tell you.
⚑ **THIS RULE IS NOW CONTRACT AND EMPYREAN GOVERNS IT** — `contract/LANE_STATUS.md` §*"Verify after EVERY
write, not only at boot"*, at empyrean `origin/main` (verified here by content at `97c4f72`; the hub cited it
as *"commit after 1489413"*, which is a coordinate rather than a SHA, so it was resolved by reading the
section, not by trusting the pointer). **The text below is this repo's PRECEDENT NARRATIVE, not a second
copy of the rule** — on any disagreement the contract wins, and the rule is not to be restated here as it
drifts. Read it at a committed revision, never through `../empyrean/`.
⚑ **n=2, AND THE SECOND INSTANCE READS SHARPER THAN A REPEAT** (aurora, verified here: contract line at
empyrean `origin/main`, carrying commit `10c87ba` — a real contract+docs commit, `--stat`-checked, and this
time the hub emitted the SHA from git rather than naming a neighbouring coordinate). **sigil wrote `closed`
the same night, an hour apart, neither lane aware of the other, both having read the warning shortly
before.** ⚑ **The part worth more than the count: we reached for TWO DIFFERENT WORDS.** That is not two
lanes making the same slip — it is two lanes independently reaching for a terminal state **the vocabulary
does not have**, and picking different plausible names for it. **The error is INVITED by the design, not
merely permitted by it**: the natural word for a finished row does not exist, because the contract's answer
is that a finished row *leaves the queue* — correct, and not what a writer's hand reaches for. A rule
against `done` would not have caught `closed`, and a rule against both would not catch `complete`.
⚑ **The skill's own boot text warned about exactly this** (*"three lanes wrote `done`, which is not in the
vocabulary, in three days"*) and I did it anyway, which is the argument for the mechanism over the warning:
a rule you have read does not fire, a curl does. Found by the aurora lane reading the console, not by me —
**this lane cannot detect its own invisibility, so it depends on a peer looking.** Worth knowing when no
peers are up: the verdict is only ever one curl away, and nothing else will surface it.


**▶ NEW BAR, 2026-08-29 — VALIDATE AN ARTIFACT AGAINST THE SCHEMA IT TARGETS BEFORE CALLING IT READY.

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1400-1426.)*
A "ready to merge" IS a completeness claim, and it is the one nobody thinks to check because it reads
as a status rather than an assertion.** Earned against this seat, on the same submission where it was
lecturing about unchecked residues.


**Corrective, and it is mechanical because vigilance already failed:** an artifact authored against a
schema is **run against that schema before it is handed over**, and the run's output is what the handover
cites — never "these conform". If the validator cannot be run from here, say so in the handover and name
what was not checked, rather than letting a confident cover note stand in for it.


**▶ NEW BAR, 2026-08-29 — A TEST THAT ASSERTS WHAT YOU *ADDED* IS STRUCTURALLY BLIND TO WHAT YOU

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1432-1480.)*
*DISPLACED*, AND A FIXED-SIZE SURFACE MAKES EVERY ADDITION A DISPLACEMENT.** Earned on the
SCREEN-HONESTY parcel, against this seat's own green test.


**Correctives, in the order they are cheap.** (1) When adding to a fixed-width surface, assert on the
**whole** rendered string, not on the field you added — `assert_eq!(rendered, full)` is the form, and
it fails for the person who adds the *next* field too. (2) **Print it and look at it** before believing
a boolean; the arithmetic here was mine and was wrong twice before the probe settled it. (3) Ordering
is a design decision on any surface that truncates: the fields that answer *"is this window lying to
me"* go first, and that ordering wants its own test with an **anti-vacuity clause** — there must exist
a width that drops a late field while keeping an early one, or the ordering test passes on a line that
never truncates at all.


**▶ NEW BAR, 2026-08-27 — A CLAIM IN OUR DOCS ABOUT A PEER'S FILE HAS A SHELF LIFE, AND NOTHING IN THIS

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1486-1504.)*
REPO CAN EVER TELL YOU IT HAS EXPIRED. MEASURED SHELF LIFE: FORTY MINUTES.** Third instance in one day,
which is what makes it a bar rather than three slips.


**The remedy, and it is cheap because it is the protocol's verified-at anchor pointed at our own
docs:** when a doc here asserts something about a sibling repo's file, **record the peer revision it
was read at, inline** — `(aeon `6e4751c3`, read 2026-08-27)`. That converts an unfalsifiable sentence
into a one-command currency check for the next reader, which is exactly what the three instances above
each lacked. **And before exporting any such claim across the fence, re-read the file at their tip** —
not the doc that quotes it.

**⚑ The sharpest form, from aeon's side of this one: a peer's warning about YOUR OWN tree is the class
you must verify before acting on, and it is the one that feels least like it needs checking** — it
arrives as help, about your own code, from someone with no motive to be wrong. They nearly briefed an
agent on our stale premise. **Our confident claim about their tree almost became their agent's
instruction**, which is the delegation corollary reaching one repo further than it was written for.


**▶ NEW BAR, 2026-08-27 — DO NOT GREP A RELEASE BINARY FOR A SHORT STRING. THE OPTIMIZER INLINES IT AS

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1524-1553.)*
AN IMMEDIATE AND IT IS SIMPLY NOT THERE AS A CONTIGUOUS SEQUENCE.** Earned by nearly reporting a
stale-binary emergency to a peer who was about to make a decision on it.


**Operational form:** to ask whether a binary contains a symbol or wire key, (1) **spawn it and call
it** — the bar already says this and it is the only answer that cannot be fooled; (2) failing that,
grep the **debug** build, or the **8-byte prefix**; (3) never read a short-string absence in a release
binary as staleness. And if a static read contradicts a live measurement, **the live one wins** —
which is our own *the receiver's already-run command outranks a confident mechanism from this seat*,
arriving with the seat on the losing side.

**▶ AND THE ONE THAT COST MORE — A NUMBER OF OURS CAME BACK AS A PEER'S AND OUTRANKED OUR OWN

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1567-1597.)*
MEASUREMENT. FIRSTHAND VERIFICATION DID NOT PROTECT US; IT IS WHAT LAUNDERED IT.** Found by aeon
against this seat, same hour, and it is the reason the absence above went unbelieved for as long as it
did.


**▶ THE COMMIT-MESSAGE BAR NOW HAS TWO INSTANCES, AND BOTH FAILED BY THE SAME MECHANISM: A LINE WRAP.**

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1609-1628.)*
Protocol bar 23 (*a commit message is a claim about a diff, and nothing checks it*) came out of this
lane on 2026-08-23, from a scripted edit that **silently failed on a line wrap** while the shell let the
commit run anyway. On 2026-08-27 aeon produced the second instance in their own tree, hours after
banking the bar — a message asserting two changes, one of which *"matched nothing, because the sentence
wraps across two lines and my pattern assumed one"* (their `c136fc3c`, corrected at `95c39449`, both
verified here as reachable ancestors of their `origin/master`; they corrected the **record**, not the
history, since the first was public).


**Two corrections to how the bar is stated, both from their instance:**
1. **`;` is a rung BELOW the `&&` the bar already calls insufficient.** Bar 23 warns that `edit && commit`
   does not protect you, since a replace matching nothing still exits zero. Theirs was weaker still —
   the commit *"sat after the failed edit in the same block rather than behind it"*, so the exit status
   was **never consulted at all.** When a block does both, the commit must be `&&`-behind the edit *and*
   behind a verification, because `&&` alone is known-insufficient.
2. **Match on a short fragment that CANNOT wrap, or read the blob back.** A multi-word prose pattern is a
   bet that the author's wrapping matches yours. Prefer a distinctive short token, and then
   `git show <sha>:<path> | grep -c` the committed blob before writing the message — the assertion in the
   message and the check that earns it are separable, and the message is the cheap one.


**▶ STATUS: PROPOSED, ACCEPTED, QUEUED — DO NOT RE-PROPOSE IT.** The hub ledgered all three sharpenings
as **Q-23** in `empyrean:docs/OVERSEER.md`'s pending protocol queue, verified here firsthand on the
pushed blob at empyrean `e27362c` (which is `origin/main` itself; `grep -c '^Q-23\.'` = 1, and the entry
carries this lane's bar-21 self-discount as stated rather than dropping it). Per the owner's batching
rule it lands **inside bar 23's text as an amendment, not as a new bar**, in the next batched protocol
pass. **Nothing is owed by this lane.** The paragraph above stays as lane-local ops guidance and is
correct whether or not the protocol pass ever runs — but a session that reads *"proposed to empyrean"*
and re-sends it is spending a peer's attention on a closed item, which is the notify-on-the-dependency
bar failing from the other end.


**▶ NEW BAR, 2026-08-29 — THE HEADLESS PLAYER RECIPE IN THIS FILE WAS INCOMPLETE, AND FOLLOWING IT PUTS A

*(The incident that earned this: `OVERSEER-LOG.md`, orig lines 1659-1672.)*
WINDOW ON THE OWNER'S DESKTOP.** Measured firsthand while discharging the two window checks; it happened on
the first launch. The banked recipe (from the SCREEN-HONESTY parcel, above) is *"the player under `xvfb-run`
with `XDG_CONFIG_HOME` pointed at a scratch `player.conf`"*. **`minifb` prefers Wayland when
`WAYLAND_DISPLAY` is set, and every lane session on this box inherits `WAYLAND_DISPLAY=wayland-0`** — so
`DISPLAY=:91` was honoured by nothing, the log said `Wayland window`, and `python-xlib` found **zero windows
on the Xvfb**. The window was on his real screen. Killed inside the minute by recorded PID.
**Corrected recipe — both guards, because the failure is silent and lands on somebody else's screen:**
`env -u WAYLAND_DISPLAY -u XDG_SESSION_TYPE DISPLAY=:N target/release/oracle-frontend --x11 …`, with your
own `Xvfb :N` whose PID you recorded. **Then verify placement before driving anything**: enumerate windows
on the display you believe you own, and treat an empty list as the finding rather than as a slow start.
**Why the note was wrong in a way nothing could surface:** it was correct *for its author's purpose* — they
were reading a screenshot, and any window would do. It is the **isolation** claim that was never true, and
the note never said which of the two it was promising. Same class as *check the vintage of the process*: a
sentence true of the file and false of the situation. *(`xdotool` is absent on this machine; `python-xlib`
0.33 is present and gives XTEST, which is what drove the keystrokes. `import` from ImageMagick grabs the
screen; `scrot`/`xwd` are absent.)*


`cd` to the absolute repo path before ANY branch operation (a persisted cwd nearly checked out
under a live agent). Fresh worktrees: `ln -s <repo>/vendor vendor`, verify 17 TestRoms entries, and
open every dispatch with a base check (commit-message string + a file that must exist). Exact-path
`git add` only; `git show --stat` per commit; no Co-Authored-By trailers. Never `cargo test | tail`.
`pkill -f`/`pgrep -f` self-match (the waiting shell's own command line contains the pattern) — bracket the first character: `pgrep -f "[c]argo test"`. ⚠ **Bracketing is not enough when the SAME command carries the literal string elsewhere** — a heredoc writing a doc that quotes the socket path made `pkill -f "[o]rc-p/o.sock"` match its own shell and kill it mid-command (exit 144, 2026-08-26). Kill by the PID you recorded at launch, not by pattern, whenever the command also contains the text. Aether sockets live under `$XDG_RUNTIME_DIR`. `/tmp` is quota'd —
free space is not the signal. The frontend is bin-only (`pub fn` with no caller = hard error).
`ls` is aliased to eza. Owner tests run `aeon/s4.debug.bin`.
A probe socket must NOT live under the session scratchpad — that path exceeds `SUN_LEN` and the
server refuses with `cannot bind the Aether socket: path must be shorter than SUN_LEN`; use a short
`/tmp/<short>` dir. The MCP shim (`oracle-old/linux-port/mcp/oracle_mcp.py`) **SPAWNs its own
`oracle-aether` by default** (private `mkdtemp` socket, `ORACLE_ROM` default `aeon/s4.debug.bin`) and
**ATTACHes only when `$ORACLE_SOCKET`/`$EXODUS_SOCKET` is set** — it does NOT use empyrean's
`resolve_socket_path()`, so do not reason about the shim from that resolver (this seat did, and was
wrong about whose emulator it was talking to).

## Coordination (when peers are up; all optional to progress)

- **seraph** (DAW): **the first FILED DEMAND against the unserved-method list (2026-08-26), and it is
  why the cutover ruling works.** Their S2 verification gate builds side B entirely out of
  `emulator_vgm_start`/`stop` → `vgm2wav`, so **S2 as banked is NOT executable against the new core**.
  Their triage, taken as given: **`vgm_{start,status,stop}` is the one that matters** (realtime and
  foreground fine; it must be deterministic enough to capture twice and compare); **`audio_spectrum` is
  explicitly NOT wanted** — do not build it on their account; **channel masks are wanted at S3, not S2**.
  **Firing condition: S1 landing.** Deliberately not filed as a dated queue row (bar 18: the dependency is
  two packages away). Treat VGM as demand-ordered-with-a-condition: of the unserved set it is the only one
  with a named consumer, a named artifact and a stated trigger. Do not pre-build it; do not renumber it
  away. That these are unserved is **machine-enforced** (`schema_conformance.rs` pins
  `SCHEMATIZED_NOT_ADVERTISED` with `assert_eq!` on the whole sorted set; `engine.rs` advertises
  `"vgm": false`); the *reason* — the synth being `cfg`-gated out — is a reading, not re-derived.
  *(Grounds, anchors and the confidence-split correction: `OVERSEER-LOG.md`.)*

## Where the detail lives

The dated `docs/2026-08-*.md` files are the arc records (handoff/recon/CR/ruling per arc — newest
first is the reading order). Today's arcs end-to-end: scanline acceptance + convention
(`…-subline-*`), CR-25/26/27 with rulings, the profiler demand/recon/deltas, the Aurora client
demand, the streaming asks. `docs/2026-08-19-subline-shipped.md` is the model handoff shape.
