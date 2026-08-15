# Handoff — the capability layer, and the plan derived from evidence (2026-08-15)

Supersedes nothing; sits after `docs/2026-08-15-handoff-play-session.md`, which covers the play session
and the audio/video fixes. **This one covers the Aether/contract arc and the work queue that follows.**

## State

| repo | tip | state |
|---|---|---|
| `oracle-next` | `0a31d09` on `m68000-microop-framework` | pushed, clean |
| `empyrean` | `18a551e` on `main` | pushed, clean |
| `aeon` | replay gate live in `test.sh`, pushed by a peer session | — |

`cargo test --workspace` **1330 passed / 0 failed / 30 legs**; clippy zero warnings on default and
`--no-default-features`; `cargo fmt --all --check` clean. **`crates/oracle-core/tests/` across the whole
session: 396 insertions, 0 deletions** — no pinned literal moved, at any point.

---

## 1. What landed in this arc

**Contract (`empyrean`, pushed and binding on the suite).** D9 amended so opaque handles are strings;
**D14** precedence (schema governs the wire, prose governs behaviour, a disagreement is a spec bug);
**D15** hosting — *descriptive* on topology, **prescriptive on registry parity**; **D16** `timingBasis`;
**D17** `droppedEvents`. §8 gained items 15–19. A schema bug was fixed that had been rejecting three of
its own catalogued methods (`z80_read`/`z80_write`/`z80_registers` — the pattern forbade digits).

> **§8 item 19 is the rule that governs all new work here:** every capability a GUI panel renders SHOULD
> exist as a bus method — and a schema entry — *before* the panel that renders it. No panel-only
> capabilities.

**The player can serve the bus** (`0a31d09`). `Host::pump(&mut sys)` lends the machine to the engine via
a `mem::swap` (measured: `System` is 1,152 bytes), answers queued commands, swaps back. `Server::spawn`
untouched; both arrangements share one accept loop. `free_run` now tracks the player's pause in both
directions. Pads merge per button. Hosted `max_run_frames` is **120, refused not clamped**, advertised as
`limits.maxRunFrames`. The Aether screen path no longer renders post-hoc — it serves the raster frame and
**reports which** (`source: "raster" | "stateRender"`). Serving is opt-in; a default launch creates no
socket and no thread.

---

## 2. ★ The plan, derived from evidence rather than precedent

Full report is in this session's transcript; the load-bearing conclusions:

**The method matters as much as the answer.** Ranking by op-name mentions in docs would have been
confidently wrong: in the eleven densest files **201 of 225 mentions are *proposed*, not executed** — 429
unchecked boxes, zero checked, placeholder operands, and one plan that pre-writes its own commit message
reporting results before the test exists. Two ops those plans call **do not exist**. The real invocation
record is **executable code** — twelve Python bus clients in `aeon/tools/`, where `run_frames` is called
34× and `hold` 18×. **Deterministic frame-stepping plus scripted input is 52 of ~90 real calls.**

**The most expensive documented gap is a missing *read*.** Unable to read VDP register `$0B`, the engine
team patched VRAM directly and reasoned backwards from the screen. The misdiagnosis stood **~8 weeks**,
survived four failed fixes, and shipped a 256-byte workaround costing ~1500–2000 cycles/frame, before
being retracted (*"⚠ THERE IS NO `$0B` PROPAGATION BUG"*). `read_vdp_registers` **is catalogued** — it was
never implemented. Eight weeks lost to an unimplemented specified method.

### Ranked capabilities (bus methods, not panels)

1. **`read{space, addr|symbol, len}`** — one read for every address space. Subsumes `read_memory`,
   `read_vram`, `read_cram`, `read_vsram`, `read_vdp_registers`, `z80_read` (**6 → 1**), and gives
   sprite/SAT decode a home as `space: sat` (it has no catalogued method at all).
2. **Deterministic scripted input over a bounded run.** The largest real-usage signal in the corpus. A
   **pad timeline is not catalogued** → change request. Retires six in-tree re-implementations.
3. **`run_until{predicate} → StopRecord`** — subsumes `step`/`step_over`/`step_out`/`run_to`/
   `run_to_scanline`/`wait_for_break`/`breakpoint_add|list|clear`/`watchpoint_add` (**10 → 1**). Core gap:
   `System::run_until_stop`'s predicate is `(pc, frame)` only and cannot express "stop when
   `Replay_Done != 0`".
4. **Per-frame value trace** — the most-requested missing instrument, which never existed. Note the shape
   ruling from their own record: **measure value *changes*, not write counts** (a census found 97% of freq
   writes were redundant re-writes of unchanged values and nearly funded two unnecessary features).
   `watchpoints.rs` already has record/count/census modes — **it is simply not on the bus.**
5. **Fault stop with a pre-clobber register snapshot** — largely already won by `crates/oracle-replay`
   (decodes from `(A7)` at `ErrorHandlerBlob+0`); needs a `fault` stop reason on the bus → change request.
6. **Per-scanline visual state, including palette-per-scanline.** Three independent rediscoveries plus our
   own currency (6 of 17 ROMs diverge post-hoc vs live). **~30 lines of sink today** — `on_scanline` and
   `on_vdp_write` already arrive through the same sink in emulated-time order. Sub-scanline needs
   `F-TRACE-VDPWRITE-MCLK` (registered, unbuilt), which unblocks `F-CRAMDOT`.
7. Build/symbol identity refused loudly (`memory_hash` is **not in §6** → change request).
8. **Screen-position → game-state attribution.** ★ **We are currently violating §8 item 19 here:**
   `pixel_attribution` exists in core, `oracle-frontend/src/pick.rs` consumes it, and it has **no bus
   method**. ~~We shipped the panel first, one day after making the rule binding.~~ **Corrected
   2026-08-15 from both repos' history: `pick.rs` landed at 06:28 and item 19 landed at 12:14–12:28 the
   same day — the panel predates the rule by about six hours, so there was neither a day nor a defiance.
   The violation as a current state stands, and it is one of four: see
   `docs/2026-08-15-pixel-attribution-bus-method.md`.** Fix this early.

**Total: ~37 catalogued methods collapse to ~11**, with the surplus concentrated in run-control and the
read family rather than spread evenly.

### Do NOT build

- **Interactive `step`/`step_over`/`step_out`.** Argued against, not merely unsupported: six stepped
  assertions all verified correct while the stored values stayed wrong. Their ruling: *"Don't try
  live-stepping — too much state, too much MCP-level uncertainty."*
- **The bus-legality detector.** Called *"highest strategic value"* in a 2026-07-20 wishlist; two years and
  ~11 hunts later, never needed once.
- **A register-write op.** Wanted twice; **both times its absence forced a better answer** — including
  `--restamp`, which round-tripped four corrupted checkpoints back to byte-identical originals.
- `log_clear` (destructive on a multi-client bus), `write_vram` in its current bypass-everything shape,
  persistent layer/channel mutes (scope them to the capture call: **4 methods → 1 parameter**), `ping`,
  `list_ops`, `debug_arbiter`.

**Distinguish "argues against" from "fails to support".** `call_stack` has zero prose mentions in either
tree but *is* called by executable clients — untested, not rejected.

### The breakpoint question, resolved carefully

"Never used" was **overstated** — it came from `oracle-next`'s hunt record only. Breakpoints paid off in
**ten executed `aeon` episodes**, three where nothing else would have worked (one read `a5` on the *first*
loop iteration; one proved a call happened *with a given argument*, which no RAM read can show). The
owner recalls a Knuckles wall-climb case; it is **not written down anywhere** — zero hits across `aeon`,
its worktrees, and `git log --all`.

The defensible finding: **breakpoint-as-deterministic-anchor is proven; breakpoint-as-interactive-session
is proven harmful.** And the confound is real — several interactive tools are *known broken*, so "nobody
used X" and "X was broken" are indistinguishable from a usage record. The test that separates them:
**would a working version have changed the outcome of any recorded hunt?**

---

## 3. Work queue, in order

1. **Three conformance items** (mechanical, unblocked now that hosting has landed):
   - checkpoint ids → **strings**. §8 item 16 records us as non-conformant until this lands. Breaking
     change to a surface with no clients — the cheapest it will ever be.
   - `reason:"runFrames"` where we emit `"step"` (CR-1 adopted, never migrated; the only test asserts it
     is *a string*). Note `press` also advances frames and also emits `"step"` — raise a small CR.
   - **A schema validator in the test loop.** Two divergences already survived review without one, and it
     immediately caught a third. Vendor or path-reference `bus-protocol.schema.json`; validate every line
     the existing `tests/common/` client receives.
2. **Fix the §8 item 19 violation** — `pixel_attribution` as a bus method, panel becomes its renderer.
3. **The MCP server** — under **D10** it is *a client of Aether*, not a fourth surface. Cheapest real proof
   the capability layer is tool-neutral before a GUI commits to it. Curate coarser than 1:1; note
   `CLAUDE.md:116` is a *naming* rule (`emulator/x` ↔ `emulator_x`), not a granularity mandate.
4. **Capabilities 1, 2, 6** from the ranking. Change requests first where the catalogue lacks a row.
5. Player build-out. Design input from the owner's use of the sibling: group by subsystem (its Debug menu
   scattered CPU across 4 groups, graphics across 3, and split audio across *two menus*); separate views
   from settings; a **command palette beats a menu at ~30 items**.

---

## 4. Owner context

- **No gamepad available.** Pad support is shipped and untestable here — record it that way; it is not an
  outstanding ask.
- **Nobody has heard the new mix levels.** SY-7 deliberately changed the FM/PSG/DAC balance (one shared
  reference level, 9-bit channel clip). That is an unvalidated perceptual change.
- The console output filter defaults to **Model 1 VA0–VA2**, chosen by ear from A/B renders;
  `ORACLE_CONSOLE_FILTER=va0|va3|off` overrides.

## 5. Ops notes that cost time

- **Agent worktrees are cut from the session-start commit, not `HEAD`.** Bit three times. Open every brief
  with "verify your base"; name the exact commit and a file that must exist.
- **Never dispatch a file-touching agent without worktree isolation.** One was, edited the shared tree
  mid-gate, and turned a gate red for reasons unrelated to the tree under test.
- `cargo test | tail` hides failures *and* returns `tail`'s exit code.
- `pkill -f "release/oracle-frontend"` matches its own shell and kills itself (exit 144). Kill by PID.
- A fresh worktree needs a `vendor` symlink or 8 frontend tests hard-fail and conformance rows silently
  skip.
- **Citations into `aeon/docs/BUGS.md` are perishable** — that file is edited in place by policy. Anchor
  on symbols or headings, not line numbers.
