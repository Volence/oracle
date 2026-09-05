# CR-Q — a save-state load at the window replaces the machine and no client is ever told

**Raised by:** oracle lane, 2026-09-05. Lane defect id **`F-STATELOAD-SILENT-REPLACE`**, booked at
`docs/OVERSEER.md` and recorded at the source in `crates/oracle-player/src/states.rs`.
**Target:** `contract/protocol.md` §11.40 (next free) — one new row in **§3 Events**, one member of the
`capabilities.events` array advertised at `initialize` (§2.1), and one §8 clause.
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule.

**Revisions everything below was read at.** Contract: `empyrean` **`origin/main` `7981617`**
(`git show origin/main:contract/protocol.md`, never the sibling working tree). Server: `oracle`
**`6d94d59`**. Two other agents are editing `crates/oracle-player` concurrently, so every line number here
is stamped with that revision and should be re-resolved by symbol, not by number.

---

## 1. The defect

`States::load` (`crates/oracle-player/src/states.rs`, `6d94d59` L180) decodes a slot file and calls
`Machine::adopt_system` (`crates/oracle-player/src/machine.rs` L268), which is `self.sys = sys` plus the
window's own resync. `oracle-frontend`'s **F4** does the identical thing inline — `commands::Cmd::LoadState`
in `crates/oracle-frontend/src/main.rs` (`6d94d59` L1850), whose swap is the bare `sys = loaded;` at L1880.
**Verified by reading both, not inherited from the report.** Both windows serve Aether
(`--aether` / `--socket`; `oracle-frontend/src/main.rs` L211-218), so both can have a socket client attached
while a person presses the key.

Neither path touches the engine. A client attached over the socket learns nothing: no event, no changed
field, no refusal. It keeps holding a machine coordinate, a latched picture and a hit ring from a timeline
that has been discarded.

### Why this is the *one* replacement that escapes, and why that is new as of today

The lane closed a neighbouring defect hours ago (`docs/lane-log.jsonl` 2026-09-05T14:22Z): `Host::pump`
snapshotted the generation counters inside itself, so anything the window dispatched through `Host::call`
between two drains was invisible to both. It was fixed by recording in `Bus::call` itself —
`SelfInflicted::absorb` (`crates/oracle-player/src/bus.rs` L258) — **deliberately not as a per-call-site
list**, because the palette is registry-derived so that no list of machine-replacing methods exists to go
stale.

That fix works, and it is exactly why this one is worth raising rather than shrugging at. **A state load is
the only machine replacement in either window that does not go through `Host::call` at all** — there is no
served method that means it (D13 rule 1 explicitly holds persistent save-states out of `checkpoint`'s scope
and says they are "a separate versioned artifact and a separate change request"). So the accounting that
was just made impossible to bypass is bypassed by the one gesture that was never routed into it. This is
not a second copy of the repair going stale; it is a door the repair cannot see.

### What actually goes wrong on the wire, enumerated

Four artifacts on the engine survive the swap. Three of them are things this contract has already ruled on
at other boundaries.

1. **Recorded watchpoint hits survive, and the frame counter rewinds.** `Bus::run_sinks` lends the
   *engine's own* watch and profiler to window-driven frames (`crates/oracle-player/src/machine.rs`
   L151-166), so hits from before the load are real engine hits stamped with the pre-load epoch's `frame`
   and cycle. This is **§11.38's confusion exactly**, at a boundary §11.38 and §11.39 did not reach —
   they enumerated over *served methods* (`reload_rom`, `reset`, `restore`), and this boundary is not one.
   §11.39's own table is the governing text: a *record* with epoch-relative fields is dropped at a boundary
   because it is uninterpretable. A window state load is a boundary by that definition and nothing is
   dropped.
2. **The latched frame survives, and `screen_generation` cannot move to say so.**
   `Engine::invalidate_screen` runs for `reload_rom`, `reset` and `restore` and not here, so
   `Engine::framebuffer` keeps preferring `last_frame` and answering `from_raster: true` — the confident
   wrong answer §6's `source` field exists to prevent. **⚑ This lane's first reading of this was wrong and
   the correction makes it worse, not better.** `last_frame` is *not* populated only by engine-driven runs:
   `Host::publish_capture` (`host.rs` L542) is exactly that seam, and **both** windows call it every frame
   whenever a client is attached (`oracle-player/src/bus.rs` L646, `oracle-frontend/src/bus.rs` L301). So
   the stale raster is available to *any* connected client, not only one that ran frames itself. It is
   self-healing while the machine free-runs — the next completed frame republishes — but a client **must
   pause the player to run anything at all** (§6's run-control state rule), so paused is the connected
   client's normal condition, and while paused nothing republishes and the abandoned timeline's picture
   stands indefinitely. And `publish_capture` **deliberately does not bump `screen_generation`**, on the
   stated ground that *"the publisher already has this image, and a bump means 'the picture moved without
   you'"* (`engine.rs` L2018) — which is precisely what a state load is, and precisely the case that
   sentence's author did not have in front of them.
3. **The profiler's shadow stack survives**, describing a machine whose returns will never come.
   `restart_profiler_sample` runs at all three served boundaries and not here.
4. **`rom_generation` does not move**, so `PumpReport::rom_changed` stays false and the in-process host runs
   none of its four repairs.

---

## 2. The signal — recommendation, and why the two obvious answers are wrong

### The brief this lane was handed said "a state load is not a ROM change." That premise is false on this server, and it matters

`PumpReport::rom_changed`'s own documentation (`crates/oracle-aether/src/host.rs` L138-150) ends:
*"Read it as 'resynchronise', not as 're-read the ROM'."* It already has three producers and **two of them
change no ROM bytes at all** — `emulator/reset` (same cartridge, power-on anchor) and `emulator/restore`
(may or may not have swapped the image; "the flag moves unconditionally rather than guessing"). So the
in-process flag is already named narrower than its meaning, on purpose, and a window state load is its
natural fourth producer. **Refusing to reuse it in-process would be the mistake, not reusing it.**

### But `rom_generation` cannot be the answer, for a reason the brief did not name

`rom_generation` is a private `Engine` field with a getter (`crates/oracle-aether/src/engine.rs` L1375,
L1978). **It is not on the wire anywhere.** No reply carries it, no event carries it, `initialize` does not
advertise it. It is read by exactly one consumer: the in-process host's `pump`/`call_reporting`
(`host.rs` L675-691, L787-789). Bumping it closes the in-process half completely and closes **nothing at
all** for a socket client. The brief conflated a server-internal counter with a client-visible signal; they
are two different layers and this CR needs both.

### Recommendation

**Half A — server-internal, needs no contract text.** Route a window state load through the same
machine-replacement accounting the three served boundaries use: bump `rom_generation`, `invalidate_screen`,
drop the recorded watchpoint hits with a count, restart the profiler sample. This is §11.38/§11.39's already
adopted rule enumerated over a fourth boundary. It requires a new `Engine` entry point (there is none today
that means "a machine was replaced beneath you"), and it is a slice, not a change request — **except that it
produces a count that has to go somewhere, which is Half B.**

**Half B — the contract change. A new event, `emulator/machineReplaced`.**

| Event | When | `params` |
|---|---|---|
| `emulator/machineReplaced` | the machine was replaced by a gesture at the window that no method call produced | `reason` (`stateLoad`), `hitsDropped` |

Plus the stamp, which §3 already applies to every event. `capabilities.events` at `initialize` gains
`"emulator/machineReplaced"`.

**Why an event and not a fourth envelope field.** §2.2 and §2.3 are deliberately closed, structurally
applied sets, and §2.3 is explicit that a client which never subscribed "will read `0` forever and needs no
branch." A third envelope field taxes every reply on every conformant server forever in order to serve one
window-only gesture. More decisively: **the client that would benefit from an envelope field is the polling
client, and the polling client is the one that already has a detector.** After a state load the D11 stamp's
`frame` usually jumps backwards, which nothing else on this bus does except a `restore` the caller itself
asked for. The client that genuinely cannot see this is the **subscribed** client, so the fix belongs in the
event stream. *(That polling detector is a mitigation, not a defence — see §4, where it is also shown to be
unsound.)*

**Why a new event and not `emulator/romReloaded`.** This is where the brief's instinct lands correctly,
one layer up from where it aimed it. §3's row for `romReloaded` reads *"a ROM (re)load completes"* and its
`params` require `path`. A state load completes no ROM load, and its `path` would have to be either absent —
breaking a required field — or the unchanged ROM path, which is a true statement answering a question
nobody asked. Worse, it is *load-bearing* for a client: §11.26's M3 obligation makes a `romReloaded` a
**symbol re-resolve trigger**, and firing one here would make every client re-resolve a listing that did not
move. That is the over-signal the `symbols_changed` doc (`host.rs` L160-166) was written to forbid, in the
form the contract can actually see.

**Why `reason` is an enum with one member today.** `reset` and `restore` are also unobservable to a
subscribed client — §11.39 said so in as many words ("there is no reset event and no restore event"), and
recorded that no consumer has asked to observe them. **This CR does not retrofit them.** The enum exists so
that the day one is asked for, the answer is an added member and not a renamed event. Adopting a
single-valued enum on purpose is the cheaper half of a choice this lane would otherwise have to unmake.

---

## 3. Vectors

Written so they can be turned into tests. `S` = a subscribed connection (`clientCapabilities.events:true`,
`initialized` sent). "the load" = a successful state load at the window, either window.

**Positive**

* **V1.** The load pushes exactly one `emulator/machineReplaced` to `S`, with `reason: "stateLoad"`, and its
  `params` carry the D11 stamp (§2.2). One event, not one per subscribed connection-visible artifact.
* **V2 — distinguishes this from a ROM reload.** The load pushes **no** `emulator/romReloaded` to `S`.
* **V3 — distinguishes this from a ROM reload, from the other side.** Across the load, `emulator/status`'s
  `romPath` is unchanged **and** a hash over the ROM bytes is unchanged. The machine moved; the cartridge did
  not. (This is what makes V2 a rule rather than a spelling preference: a client can verify the ROM is the
  same one, so an event claiming a reload would be checkably false.)
* **V4 — §11.38's shape, at this boundary.** Watchpoint hits recorded before the load are **absent**
  afterwards, and `hitsDropped` on the event counts them. Present at `0` when there were none — the same
  requiredness §11.38 gave `reload_rom` and §11.39 gave `restore`.
* **V5 — the picture, taken in the condition that actually holds.** With `S` attached and the player
  **paused** (so nothing republishes), a load happens at the window; the next `emulator/screenshot` does
  **not** return the pre-load raster with `from_raster: true`. Paused is the operative word: free-running,
  the next published frame heals it within one frame and the test would pass for the wrong reason.
* **V6.** The event reaches only connections that set `clientCapabilities.events:true` and sent
  `initialized` (§3), and `emulator/machineReplaced` appears in `capabilities.events` at `initialize`.

**Negative — the cases that must NOT fire the signal**

* **V7 — a refused load fires nothing.** A load of an empty slot, of a state whose ROM fingerprint does not
  match the running cartridge, or of a corrupt payload: no event, no hits dropped, no generation moved, and
  the machine's `state_hash` is byte-identical before and after. This is the sharpest negative available
  because `save_state::load` returns `Err` **before** any swap and there are already two tests asserting the
  machine did not move (`states.rs` `a_state_from_another_cartridge_is_refused_after_the_fingerprint_is_re_derived`
  and its empty-slot sibling). A signal that fired here would be reporting a replacement that provably did
  not happen.
* **V8 — a state *save* fires nothing.** F2 reads the machine and replaces nothing.
* **V9 — a client-driven `emulator/reload_rom` fires `romReloaded` and NOT `machineReplaced`.** One boundary,
  one signal. Without this a client that reacts to both double-counts, and `hitsDropped` would be reported
  twice for one drop.
* **V10 — `emulator/load_symbols` fires nothing.** This is the over-signal the server already tests for on
  the in-process flag (`crates/oracle-aether/tests/hosted.rs` L651: *"loading symbols raised `rom_changed` —
  the OVER-SIGNAL this parcel exists to avoid"*), asserted here on the wire signal.
* **V11 — slot selection fires nothing.** F6/F7/0-9 move which slot the keys act on and touch no machine.

V7–V11 are the half of this set that can fail. A vector list of V1–V6 alone is satisfied by a server that
emits the event unconditionally on every keypress.

---

## 4. What breaks if this is not done — bounded honestly

**Today, nothing is breaking.** Measured at 2026-09-05 on this machine: `ss -x` shows **no rows at all** for
`/tmp/oracle-aeon-owner.sock` — not a connection, not even the listener at the moment of measurement (the
brief's own earlier reading found the LISTEN and no connections; mine found neither, which is consistent
with the window having been restarted and is in any case *fewer* connections, not more). **No client is
attached to the owner's window and none has been misled.**

What has changed is the exposure, in one direction:

* the owner's window **has save states as of today's slice** — before it, this gesture did not exist there
  at all;
* another lane attaches to that socket intermittently to drive probes, so the two halves of the hazard are
  now both present in the same process, just not usually at the same instant;
* the hazard needs both halves and an ordering — an attached client, then a person pressing F4, then the
  client reading back. **Narrower for the hits** (the client must have armed a watch first) and **not narrow
  at all for the picture**, which only needs the client to be attached and the player paused, and a
  connected client that wants to run anything must pause it. Reachable rather than hypothetical.

**The failure mode when it does land is the expensive one and that is the whole argument.** Nothing errors.
`emulator/watchpoint_hits` returns hits with plausible frame numbers, `emulator/screenshot` returns a
plausible picture, the profiler returns a plausible tree. §11.38 was raised because a consumer read hits
stamped frames 397 and 655 as the current run's; the identical misreading is available here with no method
call in the log to explain it — which is *worse* to diagnose, because the client's own transcript contains
nothing that could have caused it.

**The one mitigation that exists, and why it is not enough.** A polling client can sometimes notice the D11
`frame` going backwards. It is unsound: a state saved *later* in the session and loaded moves `frame`
**forward**, and a state saved and immediately reloaded moves it by a handful of frames or not at all. A
detector that is right most of the time for a discontinuity is worse than none, because it trains the client
to trust it.

**This is not urgent and should not be adjudicated as though it were.** It is a defect with a rising
exposure and a silent failure mode, filed now because the code was in front of us and the alternative is
rediscovering it at a debugger later — which is the reason the source comment at `states.rs` L33-39 exists.

---

## 5. The cost, honestly

**Wire-visible change:** one. `capabilities.events` at `initialize` gains a member. §3's table gains a row.
Both are additive in §11.18's form — a list gains an entry, no emitted shape is widened, and no existing
field changes meaning.

**Do existing clients have to move?** No. §3 pushes events only to connections that opted in, and a client
that ignores an unrecognised event method is already required to work (the whole `events` array is a
capability list precisely so a client can subscribe to what it knows). A client that *validates* the
advertised array against a hardcoded closed set would break. **This lane did not verify that no client does,
and says so rather than asserting it**: the reference client §10 decision 1 names (`clients/python`) is not
in this repo, and the MCP shim lives in `oracle-old/`, which this worktree does not carry. It is the one
pre-adoption check this lane cannot perform, and it should fall to whoever owns those two.

**Server-side cost:** a new `Engine` entry point meaning "a machine was replaced beneath you", called from
two places (`States::load` and `oracle-frontend`'s `Cmd::LoadState`). Half A's four repairs are all existing
methods; nothing new is written, they are called from a fourth site. The event emission is the same path
`romReloaded` already uses.

**Schema cost:** an event is not a method and has no `params`/`result` fragment pair under §8 item 20 today,
so this adds no fragment. If the adjudicator wants event params schematized, say so in the ruling — that is
a larger and separate change to how §8's closure works and this CR is not the place to smuggle it.

**What this CR deliberately does not buy.** It does not make persistent save-states a served method. D13
rule 1 rules that out and this lane agrees: a `checkpoint` is a volatile in-memory coordinate and a slot file
is a durable versioned artifact, and conflating them is what D13 rule 1 forbids. This CR asks only that the
gesture be **observable**, not that it be **drivable**.

---

## 6. What would have to be true for this to be wrong

* **If the adjudicator reads a window gesture as outside the protocol's concern entirely** — the argument
  being that the contract governs a control surface and not what a person does at a keyboard. It is a
  coherent position, and §11.26's `emulator/clicked` is the precedent against it: the contract already
  carries an event whose entire trigger is *"the person at the window clicked a dot (never for a method
  call)"*. If `clicked` was right, a replacement of the whole machine is a fortiori right. But `clicked` was
  adopted under a named project (LIVE-OBJECTS) with a named consumer, and this has neither.
* **If the answer is that `oracle-player` should route the load through `Host::call` instead**, giving it a
  served method and letting the existing accounting cover it. That is the tidier design and it is Half A
  done properly rather than Half A done narrowly — but it collides with D13 rule 1 (a served state-load op
  is the persistent-save-state method D13 held out), and it still leaves Half B open, because there is no
  event for `restore` either. It would be a strictly larger CR arriving at the same wire change.
* **If no consumer ever wants it.** There is no named consumer today; the nearest is the lane that attaches
  to drive probes. This lane is not claiming a demand it does not have, and if the adjudicator wants to hold
  Half B until one exists, **Half A stands alone and should be served regardless** — the surviving hits, the
  stale raster and the stale profiler sample are wrong for an in-process reader too, and closing them needs
  no contract text at all.
