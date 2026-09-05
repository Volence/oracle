# Frontend migration S3 — save states, `.srm`, and the one door a machine replacement goes through

**Date:** 2026-09-05 · **Branch:** `worktree-agent-a25a8afdab241e500`
**Plan:** `docs/2026-09-05-frontend-migration-recon.md` §3.3 · **Inherits:**
`docs/2026-09-05-frontend-migration-s0-s2.md`, `docs/2026-09-05-frontend-migration-s2a.md`.

**No windowed binary was launched. No emulator MCP tool was touched. No process name, launch flag or
socket path was changed.** Everything below is `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt`.

---

## 0. What the brief and the plan got wrong

### 0.1 ⚑ The brief names the wrong door, and the door it names was never at risk

The brief's central warning is that *"a window-driven F5 must go through the SAME door"* as
`oracle-player`'s `bus::drain`, and that **two copies of the reload repair** is the defect this slice was
most likely to ship.

**Two copies of the reload could not have been shipped here, and the door the brief worried about was
already single before I touched anything.** `oracle-player`'s command palette is *derived from
`oracle_aether::engine::METHODS`* (`palette.rs`'s module doc: "The list is DERIVED from the registry. It
is not a list."). It therefore already offered `emulator/reload_rom`, and already dispatched it through
`Bus::call` → `Host::call` — the same in-process registry a socket client reaches. Any `F5` binding is a
keyboard alias for a call that already existed. There was nowhere for a second implementation to live.

### 0.2 ⚑ The real defect is the mirror image of the brief's, it was already shipped, and it is silent

The door that skips the repair is **the window's own**, and it has been open since parcel 2b.

`Host::pump` snapshots the three generation counters *inside itself*. That is deliberate — its own comment
says it is what keeps `Host::set_machine_info` from surfacing as a client's doing — and the unintended
half is this: **a change made through `Host::call` between two drains is invisible to both of them.** It
lands after drain N reads the counters back and before drain N+1 reads them at its start, so the delta is
zero on either side and no `PumpReport` anywhere ever mentions it.

`oracle-frontend` never met this, because it swaps its own cartridge by calling `System::load_rom`
directly and repairs its window inline in the same block. `oracle-player` dispatches **every** served
method from its palette through `Host::call`. So, measured at `17ee2c6`:

| Palette gesture | What the window did about it |
|---|---|
| `emulator/reset` | **nothing.** Audio clock and scanline capture left on a timeline that no longer exists — the sink's frame index sits above the restored one and renders *silence* until the machine catches back up. Symbol cache and ROM-path row stale. |
| `emulator/reload_rom` | **nothing**, same list, plus the `rom` row naming a cartridge that is not loaded. |
| `emulator/restore` | **nothing**, same list. |
| `emulator/run_frames` | **nothing** — the frame the client's run drew never reached the glass. |

Every one of those repairs was already written, already correct, and already running for a *client* doing
the identical thing one door over. That is exactly the CR-K shape the brief quotes, arriving from the
other side: two producers of one change, one of them unheard.

### 0.3 The plan's "flush the `.srm` first" is **unsatisfiable in this window**, and the plan does not say so

Recon §3.3 lists five ordered steps and says each exists because something was silently lost without it.
The first — *flush the pending `.srm` first* — is a property of `oracle-frontend`'s **single-actor** world:
its F5 block reads the file, flushes, and only then calls `load_rom`.

This window has two actors. A client's `emulator/reload_rom` is answered **inside `Host::pump`** and has
already zeroed the SRAM buffer before `bus::drain` executes a single line of its own. There is no pre-pump
hook, and adding one would be a change to a contract lane's surface. §2 is what was built instead.

### 0.4 ⚠ My own, and it is the one I would want read: two mutations came back green

Both are recorded in the source and in commit `[gates-fix]`.

* **A rescue-path mutation that reads exactly like a pass.** Substituting `self.path` for the carried
  path *at the write site* changes nothing — `after_replacement` re-keys `self.path` afterwards, so the
  two are equal at that moment by construction. The mutation that hits the assertion is the **ordering**
  one (move the re-key ahead of the rescue). `Pending`'s own path is load-bearing only for the orphan
  retry, which runs after the re-key, and that is a different row.
* **`Machine::adopt_system` without its resync stays green, everywhere.** §4 states it in full; it is the
  one claim in this slice that rests on structure rather than on a red mutation, and it says so.

---

## 1. What landed

### 1.1 `oracle-frontend` grows two more `pub mod`s (`efcc473`)

`save_state` (629) and `sram_file` (349) join the lib target S0 created. The binary `use`s them out of its
own lib, exactly as it already does for `present`, `pick`, `spawn` and `font`. They move **together**
because `save_state::save` writes through `sram_file::write_atomic`; splitting them would have left the
container's crash-safety in the other crate.

Both windows now write one file format, so a state written from the minifb window loads in the toolkit one
and back. Their tests moved target with them: frontend lib 62 → 82, frontend bin 287 → 267, exactly.

### 1.2 `Host::call_reporting` (`7832622`)

`Host::call` is now a wrapper over it. It takes the same four-coordinate diff `pump` takes, in the same
shape and order, **in the same file**, so the two readings of "what did that move" cannot drift.
`calls` is 1 and `deferred` is false, always.

**Nothing on the wire changes**: no field, no event, no error, no timing, and nothing a client may conclude
from a reply. It is an embedder accessor on a type the embedder already consumes.

Its test asserts **both** halves, and the second is the one worth having: the gesture's own report names
the change, and *the drain that follows it does not*. An embedder that "simplified" this away and read the
next `PumpReport` instead would fail the first half here rather than discovering it as silence at a window.

### 1.3 One door in `oracle-player` (`c9c1e1a`)

`Bus::call` records what each gesture moved into `SelfInflicted`; `drain` takes it first and fires **every
repair on the union** of that and the pump's own report. Both are carried on `Drained` separately — the
pump's report is documented as "carried verbatim" and folding a gesture's flags into it would make that
field a composition this crate invented.

The recording lives in `Bus::call` itself, the one function every panel, the transport bar and the palette
already go through — deliberately **not** a per-call-site opt-in. A per-site opt-in is a list of methods
that replace the machine, and this crate's palette is derived from the registry precisely so that no such
list exists to go stale. A method added to the engine cannot leave this window unrepaired, because nothing
here names one.

### 1.4 The battery (`battery.rs`)

The ordering rule is kept by **carrying the bytes instead of racing them**. `Battery::carry` takes the
pending image at the top of each drain; `Battery::after_replacement` writes it to *the outgoing
cartridge's* path once the machine has moved. One copy, one place, one rule, both producers.

⚑ **The `!own.rom_changed` gate on that carry is the load-bearing half.** The two producers do not land at
the same moment: a client's command is answered by the pump on the very next line, so a carry taken there
is exactly right for it — but this window's own gesture was issued in the **previous** iteration's
`build_ui`, and by drain time the buffer is already the replacement's. Re-carrying then would faithfully
snapshot the zeroed image and throw away the one that needs rescuing: the failure the mechanism exists to
prevent, arriving through the mechanism itself. Proven: dropping the gate is red on the window-driven row
and **green** on the client-driven twin.

**Which producer gets the incoming `.srm` applied is decided from `System::sram_used` and nothing else**,
because only `load_rom` clears it:

| Producer | The buffer afterwards | What happens |
|---|---|---|
| `emulator/reload_rom` | re-provisioned, **zeroed**, `sram_used` cleared | apply the new cartridge's `.srm` |
| `emulator/restore` | the checkpoint's, rolled backwards | left alone — the snapshot's battery belongs to that machine |
| `emulator/reset` | **unchanged**, as on real hardware | left alone |

Guessing from the ROM path instead would have been wrong on the loud case: **F5 reloads the same path.**

**A cost, stated rather than hidden.** `oracle-frontend` can *abort* a ROM swap whose flush failed, because
it has not swapped yet. This window cannot abort a client's. So a failed rescue keeps the bytes
(`Battery::orphan`) and retries on every subsequent iteration, loudly, until it lands. Memory holds the
only copy in the meantime — exactly as it does in the frontend's failed-flush window — and unlike the
frontend nothing has been thrown away.

The debounce is `oracle-frontend`'s 120 frames unchanged. Flushing at the top of every drain instead of
carrying would have been simpler and would have *deleted* it: the drain runs every iteration, so "flush
whatever is pending" is "flush every frame while something is pending".

### 1.5 Save states (`states.rs`) and the machine keys

Ten numbered slot files beside the ROM, through `oracle_frontend::save_state`'s container — not a second
one. `emulator/restore` exists and is **not** this: it restores a volatile in-memory checkpoint with a
server-assigned id, gone when the process ends. These are files that survive a relaunch.

The load's order, and every step of it is one of the five the recon names: flush the battery (this is one
of the two gestures whose ordering this window *does* control), take the machine through
`Machine::adopt_system` — which resynchronises the timeline in the same statement — cancel the autosave,
clear the restored dirty flag.

Keys, the incumbent's: `F1` reset, `F5` reload, `F2`/`F4` save/load, `F6`/`F7` slot, `0`-`9` direct.
`F1`/`F5` are served methods and go through `Bus::call` like every other gesture, landing in the transport
bar's own `Echo` — so `emulator/reload_rom`'s paused-only refusal is shown **in the server's own words**,
with the palette's `remedy` naming the pause control. Pausing and resuming around it on the operator's
behalf would be this window inventing two state changes nobody asked for, and a client watching would see
them.

**`Tab` is deliberately not bound to reset**, unlike `oracle-frontend`: `egui` owns it for focus traversal,
and a window whose docked panels have text boxes would make one keystroke mean both "next field" and
"reset the console".

The Screen tab grows a slot strip — `◀ slot n (occupied) ▶ save load` — naming the keys in its hover text.
The keys are the incumbent surface; the row is what makes the capability findable at all.

`on_exit` flushes the battery on shutdown, which is `oracle-frontend`'s `"on quit"` line.

---

## 2. The single door, asserted rather than intended

Nine rows, every one asserting on the machine or on bytes on disk.

| Row | What it pins |
|---|---|
| `host::a_synchronous_gesture_reports_what_it_moved_and_the_next_drain_does_not` | both halves of §0.2, in `oracle-aether` |
| `bus::one_door::a_window_gesture_is_repaired_by_the_drain_and_the_pump_reports_nothing_about_it` | the symbol cache and ROM-path row are seeded **wrong**, so "repaired" and "already right" differ; `report.rom_changed` is asserted **false** while the repair fires |
| `bus::one_door::a_window_reload_rescues_the_battery_to_the_outgoing_cartridges_file` | bytes on disk, at the outgoing path, with the incoming file asserted absent; a drain sits **between** the carry and the gesture |
| `bus::pumped::a_client_rom_reload_rescues_the_pending_battery_to_the_outgoing_cartridges_file` | the same rescue through a real socket client |
| `bus::one_door::a_reload_applies_the_incoming_cartridges_own_battery_image` | zero before, the fixture's byte after |
| `bus::one_door::a_reset_does_not_rewind_the_live_battery_to_the_file` | disk holds `0x11`, guest holds `0x5A`, asserted different first |
| `battery::the_autosave_waits_out_the_debounce_and_then_writes_once` | the file's **absence** on every frame before `AUTOSAVE_DEBOUNCE_FRAMES`, which is read and never typed |
| `battery::a_rescue_that_cannot_be_written_is_kept_and_retried_until_it_lands` | a genuinely unwritable path, then the directory is created and the retry is asserted to land |
| `states::a_slot_round_trips_the_machine_and_the_load_flushes_the_battery_first` | the machine went back **and** the pending battery reached disk first |

Plus `states`: an empty slot and a foreign cartridge are refused cleanly with the machine unmoved; the slot
wrap is over `SLOT_COUNT`; `input`: every slot has a key, no machine key is also a pad key.

### 2.1 Red-first proofs

Each mutation was applied on disk and quoted back, run, then restored with `git checkout HEAD -- <path>`
against a **committed** baseline.

| # | Mutation | Result |
|---|---|---|
| A | `drain`: `report.rom_changed \|\| own.rom_changed` → `report.rom_changed` | **RED**, all 4 `one_door` rows |
| B | `drain`: the carry's `!own.rom_changed` gate → `if true` | **RED** on the window-driven rescue; **green** on the client twin (the discriminating pair) |
| C | `Battery::after_replacement`: the `self.path` re-key moved **ahead** of the rescue | **RED**, both rescue rows |
| D | `drain`: the `battery.after_replacement(…)` line deleted | **RED**, both rescue rows |
| E | `Battery::after_replacement`: `!sys.sram_used()` → `true` | **RED**, `a_reset_does_not_rewind_the_live_battery_to_the_file` only |
| F | `States::load`: `battery.flush(…)` deleted | **RED**, the round-trip row only |
| G | `Host::call_reporting`: returns `PumpReport::default()` | **RED** in `oracle-aether` **and** all 4 `one_door` rows |
| H | `input::machine_keys`: `F5` → `Reset` | **RED**, `the_machine_keys_are_the_minifb_players_machine_keys` |

**Two that did NOT go red** — §0.4, and both are in the source beside the rows they qualify:

| # | Mutation | Result |
|---|---|---|
| C′ | `Battery::after_replacement`: write to `self.path` instead of the carried path, **at the write site** | **GREEN** — the re-key happens afterwards, so the two are equal by construction. Replaced by C. |
| I | `Machine::adopt_system`: `self.resync_after_replacement()` deleted | **GREEN**, whole suite. §4. |

---

## 3. The contract line, and where this slice stopped short of it

**Nothing here changes what `emulator/reload_rom` or `emulator/restore` means on the wire.** No field, no
event, no error, no timing. `Host::call_reporting` is additive and embedder-facing. Two things sit close
enough to the line to be worth stating plainly rather than deciding quietly:

1. **After a client's `emulator/reload_rom`, this window now applies the incoming cartridge's `.srm`.** A
   client that reloads and then reads the SRAM window sees the battery image rather than zeros. The
   *reply* is unchanged; the machine state afterwards is not. Judgement: this is the window's own file and
   the window's own repair, in the same function as the four `drain` already performed, and it makes the
   toolkit window agree with `oracle-frontend`, which has always done exactly this after its F5. It is a
   behaviour a client can observe, so it is named here rather than left to be discovered.
2. **A window save-state load replaces the machine without moving `rom_generation`**, so a client attached
   over the socket is not told: it sees the clock jump in the next stamp and nothing else.
   `oracle-frontend`'s F4 has the identical hole and always has. **Closing it would be a new signal on a
   contract lane's surface — a change request, not a slice** — so it was not built. It is recorded in
   `states.rs`'s module doc so the next person does not rediscover it at a debugger.

---

## 4. Left open

* **⚠ `Machine::adopt_system`'s resync is not gated.** Deleting `self.resync_after_replacement()` leaves
  the whole suite green. The `capture_lines() == 0` assertion is not a gate on it: the fixture drives
  `System::run_frames` directly, which never feeds `Machine::cap`, so the count was already zero. Making
  it non-zero needs a mid-frame halt, and both fixture breakpoints tried (`$00020E`, the fixture ROM's hot
  inner loop, and `$000400`) halt before the first scanline completes. The audio half is worse: its
  absence is **indefinite silence**, and observing it wants a real cpal output device in the suite. What
  stands in a gate's place is structural — the assignment and the resync are two statements of one method,
  which is the whole reason `adopt_system` exists — and it is written as weaker. **TAGGED.**
* **Nothing here was seen on a screen.** Constraint A. Three things want the owner's display and belong on
  `F-EYES-ON-PICKING`'s list: that the slot strip sits legibly above the picture at his
  `pixels_per_point`; that `F5` on a running machine shows the refusal somewhere he will look (it lands on
  the top bar's `Echo`, beside the halting alarm, which is the right place but is unverified); and that
  `F2`/`F4` feel like save/load rather than like nothing happening — their only feedback today is the
  Screen tab's `Note` and a `stderr` line.
* **The Screen tab's new strings do not reach `emulator/screen_text`.** The status quo rather than a
  regression — `build_ui` collects runs from the top bar and the palette only — but the surface just grew
  again and a save-state refusal is exactly the sort of thing a person quotes when reporting a problem.
  Recon §3.5 books the seam for S5.
* **The `.srm`/save-state file cost is not measured.** `Battery::carry` clones the SRAM buffer on every
  drain while a save is pending — at most 32 KB for the fallback map, for the ~2 s of a debounce window,
  and never otherwise. Reasoned, not measured; the instrument is the same `bench-window` arm S2a's residual
  names, and Constraint A forbids running it here.
* **`States` does not persist the selected slot across launches.** `oracle-frontend` does not either (key
  bindings and the slot both sit outside `player.conf`), and the player's persistence is `eframe` storage,
  which is S4's seam. Deliberately not slipped in here.
* **`F-STATUS-CAVEAT-NOT-ON-STRIP` is still not in the queue**, unchanged from S0-S2 §5 and S2a §4.
  `docs/lane-status.json` is deliberately uncommitted and this worktree's copy is stale.
