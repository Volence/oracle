# The debug panels — parcel 2 design

> ## ⚠ CORRECTIONS, 2026-09-03 — READ BEFORE BRIEFING 2b OR 2c OFF THIS DOCUMENT
>
> Parcel 2a is built and merged (`31d3408`). Building it surfaced that **this document's base
> `420c76d` is ELEVEN commits behind** where 2a started, not the handful assumed, and the drift is not
> only cosmetic. Found by the 2a agent, **re-verified firsthand at `31d3408` before being written here**
> — the numbers below are this seat's own measurements, not a transcription of the report.
>
> **The one that changes a design, not just a citation: §1.3 says twelve `require_paused` methods. It is
> FIFTEEN.** There are 13 call sites, and the thirteenth (`engine.rs:4767`) sits inside the shared
> `objreq_exchange` helper, which three handlers call (`:4983`, `:5051`, `:5099`) — `object_spawn`,
> `object_move`, `object_delete`. **A grep for `require_paused("emulator/` structurally cannot find a
> shared-helper site**, which is how a correct-looking enumeration came back three short. This matters to
> **P2 (Memory)** directly: the panel disables a write cell *with a reason* when the machine is running,
> and the set it must cover is larger than this document says. §1.3's other claim — that `write_vram` is
> **not** paused-gated while the other three writes are — still holds.
>
> **Also stale, verified:** §1.2's *"`object_spawn` does not exist, in either list… another lane's
> branch"* is **false** — that lane merged at `9ead69b` and all three rows are in `METHODS`. The served
> surface is **59**, not 56, so §2.5's checked partition (16+31+3+6=56) is short by three and must be
> redone before it is cited. §2.1 says `emulator/status` has nine fields; it emits **ten** unconditionally
> (`display` arrived with §11.29/CR-H) plus two conditionals. Every line number in §1.3, §9.3 and §9.4 has
> moved — `fn registers` is `:2632` not `:2598`, `set_live_pads` is `:1385` not `:843`.
>
> **Verified still correct:** the 19-vs-21 register defect itself (§9.3, and 2a fixes it), `host.rs`'s
> pause equivalence, `registers.rs:64`, `main.rs:75-89`, and §4's whole read-path argument, which 2a
> implemented as written.
>
> **New residual booked from 2a, for P2:** the panel's `rom` line shows the `--rom` argument verbatim
> while the bus's `romPath` is absolutised through `absolutise` (`engine.rs:7491`), which is a **private
> free function** — so R1's one-derivation-two-consumers is defeated by a privacy boundary, not by a
> design choice. The panel labels the row `rom` and documents it, so it claims nothing false today; P2
> should close it properly, since §11.30/CR-I already ruled that absolute paths are a property of *every*
> reply field carrying a filesystem path.
>
> **SECOND ROUND, after parcel 2b built (`64da529`) — and §2.1's own space list was wrong.**
>
> **§2.1 lists five spaces as `bus / rom+ram / vram / cram / z80`. Two of those are one derivation and one
> real space is missing.** `emulator/read {space:"bus"}` and `emulator/read_memory` both resolve through
> the same `debug_read` — verified firsthand at both call sites — so putting them in a selector as two
> entries is 2a's `A7`/`SP` wrong answer one level up: two controls for one thing, which a reader takes
> for two things that happen to agree. And **`vsram` is served by `emulator/read` and was dropped from the
> list entirely.** The shipped panel is **bus / vram / cram / vsram / z80**. Found by the 2b agent, not by
> this seat, and re-verified here.
>
> **§5.2 says "the four writes' real refusals".** There are five spaces and four write rows: **VSRAM has
> no served write method at all**, which is a different fact from "refused right now" and needs its own
> sentence in the panel, not a shared one.
>
> **A brief-level correction, mine, recorded because it was load-bearing and wrong:** I told the 2b agent
> that `Host::set_paused` "exists for" pause mirroring. **It is inert on its own** — it queues into
> `pending_free_run`, and only `Host::pump` applies it, since `Host::call` deliberately declines to (2a's
> ordering argument). Mirroring therefore needs a drain, which is why 2b pumps once at setup. Related and
> sharper: **`Engine::new` defaults to PAUSED**, so an unmirrored host does not refuse writes spuriously —
> it **accepts paused-only writes against a machine running at 60 Hz and reports success.**
>
> *The general lesson, which this repo keeps paying for: a design document's coordinates age on the same
> clock as a precedent narrative, and nothing re-reads them. Re-derive from source; where source and this
> document disagree, source wins.*

**Status: DESIGN, not built.** ~~Docs-only; no crate was touched and no `cargo` was run (another lane holds
that).~~ **Parcel 2a of it is now BUILT and merged at `31d3408`;** §§1-4 are implemented as designed, §5's
P2/P3 and the transport bar are not. Base: `420c76d` on `main`, branch `debug-panels-design`.

**What this settles:** which of the 56 served methods becomes a tab, which becomes a control, which is
on-demand and which is not surfaced at all; how a panel reads, and where the line between the two read paths
falls; the panel set parcel 2 ships and the one it does not; when layout persistence turns on.

**Method.** Every claim about our served surface below is derived from source in this worktree, with the
command shown. Where I could not establish something it says **UNMEASURED** and it is gathered in §8 — not
smoothed into a sentence. §9 lists what I found wrong, including in the owner's own ruling, in `ui.rs`, and
in parcel 1's shipped Registers placeholder.

---

## 0. The ruling this implements

`docs/OVERSEER.md` §"⚑ OWNER RULING, 2026-09-03 — WHAT GETS A TAB IN THE DEBUG WINDOW", lines 674-698:

* Default — **a capability served on the bus is reachable in the window**, not only from a tool.
* **Look at → tab. Do → not a tab** (a control in a panel, or an invoked command). **Too expensive to show
  live → on demand.**
* ⚑ **A panel must show the same answer a tool gets.** Prefer reading through the served surface over a
  private route.

§4 of this document accepts the correctness half of that last item without reservation, and argues that in
this repo the mechanism that delivers it is **one derivation under two consumers**, not one transport — which
is the contract's own D15 ruling, and is what `Host::read_instruments`' doc comment already says in terms.
The line is drawn precisely in §4.4.

---

## 1. The served surface, derived

### 1.1 The derivation, and the character class

`crates/oracle-aether/src/engine.rs` holds `pub const METHODS: &[MethodSpec]` (line 234). `Engine::dispatch`
(line 1870) resolves a request by `METHODS.iter().find(|m| m.name == method)` and calls `spec.handler`, and
`initialize` builds its advertised `methods` array from the same slice (line 1918). **So the advertised list
and the dispatched list are the same object — they cannot drift, structurally, not by discipline.** That is
worth stating because it is the one place in this design where I did not have to check two lists against each
other.

```
$ grep -oE 'name: "[A-Za-z0-9_/]+"' crates/oracle-aether/src/engine.rs | sed 's/name: "//;s/"//' | sort -u | wc -l
56
$ grep -c 'handler: Engine::' crates/oracle-aether/src/engine.rs
56
```

56 names, 56 handlers, and no name outside the `emulator/` namespace.

**The character class matters and here is the proof, not the warning.** The `EVENTS` array sits at
engine.rs:616-620. Run both classes over it:

```
$ sed -n '600,625p' crates/oracle-aether/src/engine.rs | grep -oE '"emulator/[A-Za-z0-9_]+"'
"emulator/stopped"
"emulator/resumed"
"emulator/romReloaded"

$ sed -n '600,625p' crates/oracle-aether/src/engine.rs | grep -oE '"emulator/[a-z_]+"'
"emulator/stopped"
"emulator/resumed"
```

The narrow class drops `emulator/romReloaded` **silently** — it returns two rows and no error. Every
enumeration in this document uses `[A-Za-z0-9_/]`.

### 1.2 What the catalog has that we do not serve

The vendored contract schema is `crates/oracle-aether/tests/contract/bus-protocol.schema.json` (vendored — I
did not read `empyrean`'s live working tree; see the standing `F-SCHEMA-READS-LIVE-EMPYREAN` defect).

```
$ grep -oE 'name: "[A-Za-z0-9_/]+"' crates/oracle-aether/src/engine.rs \
    | sed 's/name: "//;s/"//' | sort -u > /tmp/served.txt
$ python3 -c "
import json,re
s=json.dumps(json.load(open('crates/oracle-aether/tests/contract/bus-protocol.schema.json')))
sch=set(re.findall(r'emulator/[A-Za-z0-9_]+', s)) - {'emulator/stopped','emulator/resumed','emulator/romReloaded'}
srv=set(open('/tmp/served.txt').read().split())
print(len(srv), len(sch)); print(sorted(sch-srv)); print(sorted(srv-sch))"
56 67
['emulator/audio_spectrum', 'emulator/clicked', 'emulator/get_channel_states', 'emulator/log_clear',
 'emulator/log_tail', 'emulator/ping', 'emulator/set_channel_enabled', 'emulator/vgm_start',
 'emulator/vgm_status', 'emulator/vgm_stop', 'emulator/z80_registers']
[]
```

**Nothing is served that the schema does not describe** — the direction that would be a contract violation is
empty. Eleven catalogued rows are unserved; all eleven are out of parcel 2's scope and none is a panel this
design proposes.

**`object_spawn` does not exist**, in either list. The owner's ruling names it as its worked example. It is a
*concurrent lane on another branch* (`docs/lane-status.json:5-6,23`: "Building the spawn/move/delete feature…
branch worktree (spawn serve)"). §7 says where it lands when it merges; parcel 2 must not assume it.

### 1.3 The pause rule, which decides more of this design than anything else

```
$ grep -n "require_paused(" crates/oracle-aether/src/engine.rs
2495:    fn require_paused(&self, method: &str)      <- the definition
2616: run_frames      2643: run_to        2761: run_to_scanline
2863: step            2892: step_over     2912: step_out
3032: write_memory    3251: write_cram    4500: z80_write
4798: press           4856: play_input    5233: reload_rom
```

**Twelve methods refuse a running machine with `-32005 machineRunning`.** Hosted, "running" means *the
player's own loop is advancing the machine* — `Host::set_paused` (host.rs:284) mirrors the player's pause
state onto the bus, and host.rs:274-282 states the equivalence outright: "an un-paused player **is** a
free-running bus." So while the human is playing, those twelve are refused **to the window's own panels too**
if the window reaches them by the served surface. That is not a bug to route around; it is the contract, and
§4 builds on it rather than fighting it.

Note the asymmetry it exposes: `write_memory`, `write_cram` and `z80_write` are paused-only; **`write_vram` is
not** (engine.rs:3412 has no `require_paused`). A memory panel with an edit box will therefore let a user poke
VRAM mid-frame and refuse the identical gesture on work RAM. §7 books this as an open question, not a fix.

---

## 2. The verdict table — all 56

Four verdicts, per the ruling. **"not surfaced" is argued in every row it appears in.** Where a method's
natural home is an existing panel rather than one of its own, the row names the panel.

### 2.1 Tabs — things you look at

| method | tab | reads |
|---|---|---|
| `registers` | **Registers** | the 68000 file. §9.3 — parcel 1's placeholder does *not* agree with this method today. |
| `status` | **Registers** (header strip) | pc/sp/sr, `symbolAtPc`, `romPath`, `romBytes`, `symbolCount`, `frameToken`. Nine fields is a header, not a tab; a tab holding nine key/values beside a tab holding the same pc/sp/sr is two panels disagreeing waiting to happen. |
| `read`, `read_memory`, `read_vram`, `read_cram`, `z80_read` | **Memory** | one hex view with a space selector: bus / rom+ram / vram / cram / z80. Five methods, one panel, because they are five spellings of one gesture and five tabs would be five scroll positions to keep in your head. |
| `sprites` | **Sprites** (or a section of Memory in a thinner parcel) | the SAT in slot order, with the parse cap and the stale-cache flag. |
| `object_list` | **Objects** | the live pool. |
| `player_state` | **Objects** (section) | the player pool, inactive slots included. Same decoder, same layout, same refusal — a separate tab would be the same table with a different filter. |
| `object_slot` | **Objects** (row expansion) | one addressed slot; this is what a row click shows. |
| `get_profiler`, `get_profiler_frames` | **Profiler** | armed state + the accumulated sample. |
| `breakpoint_list` | **Breakpoints** | the armed set with hit counts. |
| `watchpoint_list`, `watchpoint_hits` | **Watchpoints** | the armed watches and the hit log. `watchpoint_hits` is polled and non-destructive (its own summary says so), which is exactly what a repainting panel needs. |

**Sixteen methods → seven tabs**: Registers, Memory, Sprites, Objects, Profiler, Breakpoints, Watchpoints.
Sprites is the one §5 puts last and folds into Memory under a thinner parcel.

### 2.2 Controls — things you do

| method | control, and where |
|---|---|
| `step`, `step_over`, `step_out` | transport bar — three buttons. |
| `run_frames`, `run_to`, `run_to_scanline` | transport bar — "run N", "run to ⟨addr\|symbol⟩", "run to line N". `run_to`'s `symbol` param is why the box takes a symbol, not only hex. |
| `pause`, `resume` | one toggle. **Already exists** as the player's own pause; hosting the bus makes them the same flag (`Host::set_paused` / `Host::is_paused`), it does not add a second one. |
| `reset`, `reload_rom` | transport bar, destructive-side. Neither is `require_paused`, so both work while playing. |
| `checkpoint`, `restore`, `checkpoint_drop`, `checkpoint_list` | a slot strip — 8 volatile slots (`capabilities.checkpoints.cap`), each a save/load/drop. `checkpoint_list` is the strip's own content, which is why it is a control and not a tab: a list of eight labelled buttons **is** the list. |
| `set_profiler` | arm/disarm checkbox in the **Profiler** tab head, with the `callers`/`perFrame` flags beside it. Arming resets the sample — the checkbox must say so. |
| `breakpoint_add`, `breakpoint_set_enabled`, `breakpoint_clear` | the **Breakpoints** tab's own rows: an add box, a per-row enable checkbox, a per-row ✕ and a clear-all. |
| `watchpoint_add`, `watchpoint_clear` | ditto in **Watchpoints**. |
| `write_memory`, `write_vram`, `write_cram`, `z80_write` | edit-in-place in the **Memory** tab. Three of the four are paused-only (§1.3) — the cell must be *disabled with a reason*, never silently inert. |
| `memory_hash` | a "hash this range" button on the Memory tab's range selector. It is a read, but you invoke it for a range you chose; nothing repaints it. |
| `state_hash` | a button in the Registers/status strip. Same argument. |
| `screenshot` | a button. It writes a file and replies with the path; that is a command, not a view. |
| `get_layer_states`, `set_layer_enabled` | four checkboxes on the **Screen** tab. The getter is the checkboxes' own state — a "layer states" tab would be a tab holding four booleans. **These must move `Bus::layers()`, the engine's mask, not a copy** (`Bus::layers`' doc in bus.rs: *"There is nothing here to drift apart from — this is a lend, not a mirror"*). |
| `lookup_symbol` | it *is* the address box. Every address entry in Memory / Breakpoints / `run_to` accepts a symbol, and that acceptance is this method. A symbol-search tab is a worse address box. |
| `load_symbols` | a file action + a launch argument. **It is also a hard prerequisite for the Objects tab** — see §5.3. |

### 2.3 On demand — too expensive, or per-gesture

| method | why, and what invokes it |
|---|---|
| `scanlines` | ~438 KB per frame, derived below. Never docked. Invoked from a "capture rows" action; the reply goes to a file or a scratch view. |
| `pixel_attribution` | per-click, not per-frame. Its surface is **clicking a dot in the Screen tab**; the answer appears in a side pane. This is exactly the shape the ruling prescribes and exactly the shape `pick.rs` already ships in the minifb player. |
| `object_at` | same gesture, same click. One click can answer both. |

**The 440 KB figure, derived** (engine.rs:4231-4268 is the handler). Each row is
`{"line":N,"width":320,"rgb":"0x……"}` with `width × 3` bytes hex-encoded:

```
224 rows × 320 px × 3 bytes × 2 hex chars = 430,080 chars
+ "0x" per row                            =       448
+ per-row JSON scaffolding (~35 B × 224)  =     7,840
                                            ---------
                                            ≈ 438,368 B  ≈ 428 KiB
```

The ruling's "~440 KB of JSON per frame" is correct. At 60 Hz that is 25 MB/s of `serde_json` allocation to
show a picture the Screen tab is already showing from a texture.

### 2.4 Not surfaced — argued

| method | why not |
|---|---|
| `wait_for_break` | **Deprecated by the `stopped` event in its own summary** ("deprecated by the `stopped` event; see §6 D6"). Hosted, the window is the run loop: it learns of a halt *synchronously*, from `break_observed(brk)` on the sink it fed, one frame before any event could reach it. A polling control here would be a slower path to something the panel already has. |
| `press`, `play_input` | The window has a keyboard, and `input::decide` (parcel 1) already delivers it. Both are `require_paused`, so a "press A" button would refuse for the whole time the game is playable — the state in which a human would reach for it. A macro/timeline editor is a real feature and it is not this parcel's. |
| `hold`, `release_all` | Not as controls, for `press`'s reason. **But their effect must be visible** — see §9.4: a client's held set ORs into the human's pad (`Bus::merge_held`), and today a human whose character walks left forever has nothing on screen that can tell them a client is holding Left. One read-only line in the status strip, sourced from `Host::held(0)`/`held(1)`. That is not a tab and not a control; it is a field. **✅ BUILT (`HELD-PADS-PLAYER`)**, together with the merge itself, which §5.6.2 had booked as undone — see §9.4. |
| `screen_text` | **The window is this method's producer, not its consumer** (`Host::set_screen_text`; engine.rs:1355). A panel rendering it would render the window's own chrome back at itself. It exists so a *client* can read our glass. |
| `restore` — the *implicit* case | Listed as a control in §2.2; noted here because `PumpReport.rom_changed`/`timeline_moved` mean every panel's cached state is stale after one. Panels must derive from the machine each repaint rather than cache across a pump. |

### 2.5 The partition, checked

The four verdicts must be a *partition* of the 56 — no method twice, none missing, none invented. Asserting
that from a reading of my own table is the failure mode this repo names repeatedly, so it is checked
mechanically against §1.1's derived list:

```
counts 16 31 3 6 sum 56
dupes []
missing from my table: []
invented: []
```

**16 tab-content · 31 control · 3 on-demand · 6 not-surfaced = 56.** The six not surfaced are `press`,
`play_input`, `hold`, `release_all`, `wait_for_break`, `screen_text`, each argued in §2.4. `restore` is
counted once, as a control.

---

## 3. What the player is today — the honest starting point

`crates/oracle-player` (parcel 1, `db1c2c7`) **does not depend on `oracle-aether` at all**:

```
$ grep -rn "oracle_aether\|oracle-aether" crates/oracle-player
(no matches)
$ grep -n "^oracle-core" crates/oracle-player/Cargo.toml
oracle-core = { path = "../oracle-core", features = ["synth"] }
```

`Machine` owns a bare `System` (`machine.rs:34`) and runs it with `run_frames_with_sink(1, &mut self.cap)` —
**no `Observe` wrapper, no watchpoint sink, no profiler sink, no breakpoint sink.** There is no `--aether`
argument (`main.rs:75-89` is the whole `Args` struct) and no symbol loading.

`crates/oracle-frontend` — the minifb player — **already does all of it**, behind an optional `aether`
feature: `bus.rs` hosts `oracle_aether::host::Host`, merges pads, publishes the frame, feeds `run_sinks`, and
draws six lenses (`src/lens/`, 6,281 lines) from the *same* instruments the bus serves.

So parcel 2 is not inventing this seam. It is porting a proven one into a crate that has none of it.

---

## 4. The read path

### 4.1 The three candidate routes, and which of them exist

| route | exists today? |
|---|---|
| **(a) Direct** — the panel reads `System`/`Vdp`/`Registers`/`decoders` in-process | Yes. Parcel 1 already does it (`Machine::cpu_regs`), and `oracle-frontend`'s lenses do it for six panels. |
| **(b) In-process dispatch** — the panel calls `Engine::dispatch` synchronously, no socket, no queue | **No.** `Host::engine` is a private field (host.rs:173); `EngineMsg` is `pub(crate)` (server.rs:178); `Host::pump` drains an `mpsc::Receiver` fed *only* by socket-accept threads (`spawn_accept`). There is no public in-process call path. ≈10 lines would add one. |
| **(c) Over the wire** — the panel opens a socket to its own process | Possible, and it is the thing the contract names as the anti-pattern. |

The ruling's "read through the served surface" reads most naturally as (b) or (c). (c) is settled below. (b)
does not exist yet, and §4.4 recommends adding it — for the *right* half of the surface.

### 4.2 Route (c) is already ruled against, by the contract, in this repo

`crates/oracle-frontend/src/pick.rs:645-655` carries the invariant the owner's ruling cites — and it carries
the resolution too, which the ruling's framing inverts:

> **This panel and the `emulator/pixel_attribution` bus method must never disagree** … §8 item 19 mandates the
> *capability* on the bus; **D15 argues explicitly against the panel reaching it through a socket round-trip
> per repaint** ("an in-process GUI is a consumer of the same registry, not a second server"), and our
> `Host::pump` arrangement makes that worse — a click would have to enqueue a command and wait a frame to
> answer a question it can answer synchronously. **So the panel keeps calling core**, and *this test* is what
> makes "one implementation under both consumers" checkable rather than merely intended.

D15 verbatim, quoted in `docs/2026-08-15-pixel-attribution-bus-method.md:534-543`:

> *An in-process GUI is a consumer of the same registry, not a second server.* … it buys a process boundary
> and a wire round-trip per repaint, and couples the view to the wire format at the one moment when not being
> coupled is free.

**So "routing panels through one surface makes parity true by construction" is not what the `pick.rs`
invariant demonstrates.** What makes it true by construction there is *one implementation* —
`oracle_core::render::sprite_tile_at`, moved into core precisely so both consumers call it — with a parity
test (`pick.rs`'s `mod bus_parity`) as the checker. The transport is not the mechanism, and never was.

`Host::read_instruments`' own doc comment (host.rs:441-447) says the same thing without hedging:

> A host draws its panels from *these* — **the same instruments its loop feeds and the bus serves, so a local
> readout and a client's reply cannot disagree.**

### 4.3 What route (b) or (c) would actually cost, stated honestly

Not hypothetically — these are the numbers a reader should weigh against the parity gain.

1. **Serialization per repaint.** `registers` (engine.rs:2598) builds a 13-entry `Map` of hex `String`s; that
   is cheap and I would not argue against it on cost. `object_list` builds a `Vec<Value>` over the whole live
   pool per call. `scanlines` is ≈438 KB (§2.3). Cost is not uniform across the surface and a blanket "route
   everything" rule prices the cheap panels at the expensive one's rate.
2. **The 4 ms budget does not bound a single call.** `host.rs:98-99`, verbatim:

   > It is checked *between* commands, never inside one: a command that has begun always runs to completion,
   > because a half-executed `run_frames` is not a thing the protocol can describe.

   So one docked panel calling `scanlines` inside `update()` spends its whole cost inside that frame,
   `pump_budget` notwithstanding. The budget protects against *many* commands, never against *one big* one.
   That is exactly why the ruling's on-demand category is load-bearing and not merely tidy.
3. **Route (c) additionally costs a frame of latency per answer**, because `Host::pump` is drained once per
   iteration at the top of the loop. A click asking "what is this dot?" would be answered on the *next*
   frame, against a machine that has moved. For `pixel_attribution` that is not slow, it is *wrong*.
4. **The pause rule (§1.3) refuses 12 of 56 while the window is playing.** A transport bar routed through the
   bus would be told `-32005 machineRunning` by its own process for `step`, `run_to` and every write. This is
   handleable (pause first, then call — which is what a human means by clicking Step anyway) but it must be
   *designed*, not discovered.
5. **Parity is not obtained for free by routing.** A panel that goes through `dispatch` gets the handler's
   answer for the fields the handler emits — and gets *nothing* for a field the handler does not emit. It
   also couples the panel to the JSON shape, so a schema change that a client absorbs by version-negotiating
   becomes a panel that displays `null`.

### 4.4 The decision — a hybrid, and here is the line

**Per-frame panel bodies read direct (route a). Per-gesture commands and writes go through a new synchronous
`Host::call` (route b). Route (c) is never used.**

The line is not "reads vs writes" and it is not "cheap vs expensive". It is:

> **Anything a panel repaints at 60 Hz reads a shared derivation directly. Anything a human invokes once goes
> through `Engine::dispatch`, so it gets the tool's exact answer, the tool's exact refusal, and the tool's
> exact error text.**

Three rules make that safe, and they are the deliverable, not the aspiration:

**R1 — one derivation, two consumers.** Any value a panel and a handler both need lives in exactly one
function. Where the panel is on an unconditional path, that function goes in `oracle-core` (this is what
forced `sprite_tile_at` there — `oracle-aether` is an *optional* dependency of `oracle-frontend`, so shared
code cannot live in it). **`oracle-player` has no such constraint**: it can depend on `oracle-aether`
unconditionally, which means `oracle_aether::decoders` — already fully public, `derive()`, `DecodedRecord`,
`to_json()` — is directly reusable by an Objects panel. That panel and `emulator/object_list` would run *the
same decoder over the same bytes*. Parity by construction, with no wire.

**R2 — one instrument, two readers.** Breakpoints, watchpoints, the profiler and the layer mask are **owned by
the `Host`**, never duplicated in the player. The panel reads them through `Host::read_instruments()` (shared
borrow — "a panel cannot move a number a client is gating on") and `Bus::layers()`. This is not a parity
convenience; it is ownership. Two breakpoint lists is the drift §8 item 19 exists to prevent.

**R3 — a parity test per pair.** `pick.rs`'s `mod bus_parity` is the template: it lives in the crate that can
see both sides. For parcel 2 that crate is `oracle-player` itself, which will depend on both `oracle-core` and
`oracle-aether`.

**And the new piece: `Host::call`.** Add, in `oracle-aether/src/host.rs`:

```rust
/// Answer one command synchronously, in-process, against the caller's machine — the same swap-and-dispatch
/// `pump` performs, without the queue. This is what an in-process GUI is (D15): a consumer of the registry,
/// not a second server. It is NOT a way around `pump` for socket clients.
pub fn call(&mut self, sys: &mut System, method: &str, params: &Value)
    -> (Result<Value, RpcError>, Map<String, Value>)
```

It is ~10 lines (`swap_system`, `dispatch`, `stamp`, `swap_system` back) and it removes every one of D15's
stated objections *except* the JSON coupling: no process boundary, no socket, no wire round-trip, no
one-frame latency. What it does not remove is cost-per-call, which is why it is confined to per-gesture use.

This is a hybrid, and it should be read as one. It **grants the ruling's correctness half in full** — every
panel shows the same answer a tool gets, and every command produces the tool's own reply and refusal — and it
**declines the transport half**, which the contract already declined for reasons that are stronger here than
where they were written, because our `Host::pump` makes the round-trip a frame long.

---

## 5. The panel set for parcel 2, prioritised

The ordering is by **prerequisite depth**, not by desirability, because three of the six panels share one
large prerequisite and three share none.

### 5.1 P1 — Registers + Status (one tab) — *ship first*

**Reads:** `Machine::cpu_regs()` (already there) + rom path / symbol count / `symbolAtPc`.
**New plumbing:** none. **New dependency:** none for the register half.
**Why first:** it is the only panel whose data path already exists, it retires the "(placeholder)" title the
owner is looking at, and **it fixes a live parity defect** (§9.3). It is the cheapest possible demonstration
that R1 and R3 work.

### 5.2 P2 — Memory (five spaces, hex, read + gated write)

**Reads:** direct — the bus read path, `sys.vram()`, `sys.vdp().cram()`, `vsram()`, the Z80 window.
**Writes:** through `Host::call`, so the panel inherits the four writes' real refusals verbatim.
**Prerequisite:** a symbol table, so the address box accepts a symbol (`lookup_symbol`'s surface, §2.2). The
player has none — it has no `--symbols` and no auto-discovery. `oracle-frontend/src/symbol_file.rs` (11 KB)
is the port.
**Why second:** it is the panel a debugger is *for*, and it pays the symbol-table cost that P3 also needs.

### 5.3 P3 — Objects (`object_list` / `player_state` / `object_slot`)

**Reads:** `oracle_aether::decoders` — the public module the handlers themselves use. R1 in its purest form:
one decoder, two renderers.
**Hard prerequisite:** the same symbol table, and it is not optional. `decoders::derive(None)` refuses
outright (`decoders.rs:367-374`): *"no symbol table is loaded, so no object layout can be derived … This row
refuses rather than decoding from a guessed base address."* **A player launched without a `.lst` has no
Objects tab, and the tab must say that in those words rather than showing an empty table.**
**Why third:** highest game-facing value in the suite, near-zero parity risk, and no run-loop change.

### 5.4 P4/P5/P6 — Breakpoints, Watchpoints, Profiler — *one prerequisite, and it is the run loop*

> ### ⚠ **THIS SUBSECTION WAS WRITTEN BEFORE PARCEL 2 LANDED, AND TWO OF ITS THREE COSTS WERE ALREADY PAID.**
>
> It is kept as written because its *conclusion* was right — the three panels share one run-loop change and
> that change re-opens parcel 1's measurement — but it prices work parcel 2b had already done, and a reader
> costing parcel 3 from this list would double-count it. What it gets wrong:
>
> * **"hosting the `Host` in `oracle-player`"** — done in parcel 2b. `oracle-aether` is an unconditional
>   dependency of `oracle-player`, `crates/oracle-player/src/bus.rs` owns a `Host`, and `Host::call`
>   (`crates/oracle-aether/src/host.rs:533`) is what the Memory and Objects tabs already talk through.
> * **"threading the pause flag both ways"** — the outbound half shipped in 2b as `Bus::mirror_pause`, with
>   four tests behind it. Parcel 3 added the inbound half and made the drain unconditional.
> * **"merging held pads"** — *not done, and not this parcel's.* `Host::set_live_pads` / `Host::held` are
>   still unwired in `oracle-player`; a client's `emulator/hold` does not compose with the keyboard there.
>   Booked below as an open item rather than quietly closed.
> * **The pacing re-measurement was the one cost this section priced correctly, and parcel 3 paid it.**
>   See **§5.6**.
>
> The parcel-1 quotation ("hosting it would put the 4 ms pump budget inside the frame") is worth reading
> against §5.6's measured answer: the drain's **median cost is 0.000 ms and its worst across 9,000 measured
> iterations is 0.013 ms**. `HostConfig::pump_budget` is a *ceiling on a drain that has queued work*, and an
> unserved player never queues any.

All three ride the same seam: `Host::run_sinks(resume_pc)` returns the `(Observe<&mut Watchpoints>,
Observe<&mut Profiler>, BreakStop)` triple the player's own run must carry, and the halt must come back
through `break_observed` → `Host::record_break`. Before parcel 3 `Machine::step` ran
`self.sys.run_frames_with_sink(1, &mut self.cap)` with no wrapper at all. Adding it meant:

* hosting the `Host` in `oracle-player` (parcel 1 excluded this deliberately — pacing doc line 446: *"hosting
  it would put the 4 ms pump budget inside the frame that this parcel is trying to prove"*);
* threading the pause flag both ways, merging held pads, publishing the capture — bus.rs's four "conflicts";
* **and changing the inner loop parcel 1 measured.** Parcel 1's numbers (emulate/audio/convert/upload/ui
  buckets) were taken against a loop with no `Observe` wrappers and no pump. Adding them invalidates that
  measurement and it would have to be retaken.

That last point is the reason for the recommendation below and not an aside.

### 5.5 Recommendation

> **Parcel 2 = P1 + P2 + P3, read-only-plus-gated-writes, with `Host::call` and the transport bar. Parcel 3 =
> P4 + P5 + P6, together, because they share one run-loop change and that change re-opens parcel 1's
> measurement.**

Splitting P4-P6 across parcels would pay the run-loop cost once and bank it three times, which reads as
progress and is not. Keeping them together also keeps the re-measurement to one event.

A thinner parcel 2, if the owner wants it thinner: **P1 + P3.** They share no prerequisite with each other
except the symbol table P3 needs, and together they cover "look at the CPU" and "look at the game", which is
most of what a Sonic-engine debugger is used for.

---

### 5.6 Parcel 3 — the run-loop seam, and the re-measurement it owed

> ### ✅ **BUILT, on `parcel/panels-3-stopping`.** The seam only; the three tabs are the next parcel's.

**What shipped.** `Machine::step` now carries `Bus::run_sinks(resume_pc)` on every emulated frame — the
engine's own `Watchpoints` and `Profiler` wrapped in `Observe`, plus the breakpoint sink **bare** — and the
halt returns through `break_observed` → `Bus::record_break`. `Bus::mirror_pause` drains once per iteration,
**unconditionally**, after the frame; `Loop::iterate` adopts `Bus::is_paused()` at the **top**, before the
governor's tick, which is what makes a halt stop the player rather than merely be recorded. The completed
frame goes out through `Bus::publish`. Nothing is duplicated: there is one breakpoint list, one watch, one
profiler and one layer mask, and they live on the `Host` (R2).

The transport bar (pause / resume / step) is a **control, not a `Tab`**. `LAYOUT_VERSION` stays at **1** and
no stored layout is discarded. Every gesture goes through `Host::call` and the bar renders the tool's own
reply and its own refusal, branching on `Answer::reason()` — `emulator/step` against a running player is
refused `-32005 machineRunning` by the handler, and that is the sentence a human sees.

### 5.6.1 The measurement, retaken

Same rig as `docs/2026-09-02-player-pacing-design.md` §4: `--mode bench-cpu`, 75 s, `aeon/s4.debug.bin`, a
real audio device at gain 0.0, no window and no GPU. **Four separate process invocations, reported
separately and never averaged.** BEFORE is the release binary built from `f2ddda9` (the parcel-2 tip),
preserved and re-run in this same session so both halves are one sitting.

**The condition of the machine, either side of every run.** `pgrep -c -f "[c]argo"` was **0** before and
after all four — the peer `sigil` lane's `cargo test --release --workspace` had finished, and one earlier
run taken while it was still going (load average 22.7) was **discarded rather than reported**. Everything
else was the owner's live session: Vivaldi across three processes at ~46 %, Discord, `kwin_wayland`, Steam
helpers throughout, plus what is named per run below. Load average **2.2–2.7 on 16 cores** for all four.

| | **BEFORE 1** | **BEFORE 2** | **AFTER 1** | **AFTER 2** |
|---|---|---|---|---|
| **emulated frames/s** | **60.038** | **60.036** | **60.038** | **60.038** |
| presented frames/s | 59.998 | 59.996 | 59.998 | 59.998 |
| **frame period, median** | **16.665 ms** | **16.665 ms** | **16.666 ms** | **16.665 ms** |
| frame period, p95 / p99 | 16.884 / 17.226 | 16.867 / 17.143 | 16.969 / 17.317 | 16.899 / 17.348 |
| **frame period, WORST** | **22.591 ms** | **22.516 ms** | **23.536 ms** | **22.745 ms** |
| governor rebases | 0 | 0 | 0 | 0 |
| worst lateness | 6.451 ms | 6.438 ms | 10.334 ms | 7.683 ms |
| iterations running 2 frames | 3 (0.067 %) | 3 (0.067 %) | 3 (0.067 %) | 3 (0.067 %) |
| iterations running 0 frames | 0 | 0 | 0 | 0 |
| **audio starvations, steady** | **0** | **0** | **0** | **0** |
| **audio producer drops** | **0** | **0** | **0** | **0** |
| leanest ring | 6702 (76.0 ms) | 6584 (74.6 ms) | 6584 (74.6 ms) | 6762 (76.7 ms) |
| load on the box | browser + chat only | browser + chat only | one `sigil` at 98–100 % | one `sigil` at 78–95 % |

Per-iteration cost, **medians**, milliseconds (n = 4500 each):

```
part            BEFORE 1  BEFORE 2  |  AFTER 1  AFTER 2
emulate            2.653     2.744  |    2.694    2.744
audio              0.001     0.001  |    0.001    0.001
convert            0.056     0.056  |    0.035    0.035
tex-upload         0.005     0.005  |    0.005    0.005
ui-build           0.140     0.140  |    0.149    0.149
tessellate         0.025     0.025  |    0.027    0.027
bus-pump               —         —  |    0.000    0.000     <-- NEW: this parcel's own drain
CPU TOTAL          2.854     2.946  |    2.887    2.937
period            16.665    16.665  |   16.666   16.665
```

**Reading it, including the parts that moved.**

* ~~**The seam is free at the median and nearly free at the tail.** `bus-pump` — one `Host::set_paused`
  plus one `Host::pump` per iteration — is **0.000 ms median, 0.001 ms p99, 0.003 / 0.013 ms worst** across
  9,000 measured iterations. An unserved player queues nothing, so the drain is a `try_recv` on an empty
  channel plus two ~1 KB `System` moves. `HostConfig::pump_budget`'s 4 ms is a ceiling on a drain that has
  work; this one never does.~~
  > ⚑ **The NUMBERS still stand; the REASONING under them was retired by `PLAYER-SERVE`.** *"An unserved
  > player queues nothing"* was a true statement about a player that could not bind a socket, and it is a
  > statement about a build that no longer exists — `--aether` / `--socket PATH` / `ORACLE_AETHER=1` now
  > bind one, and a bound socket can queue. The 0.000 ms above is therefore the **unserved control** and
  > nothing more; it is not the player's `bus-pump` cost with the bus turned on. §5.8.1 is the retake, and
  > it re-establishes this row for the served configurations rather than assuming the reasoning carries.
* **`ui-build` rose 0.140 → 0.149 ms at the median, and that is attributable**: it is the transport bar —
  two buttons, an armed-instruments label and the echo — drawn every frame. **+0.009 ms is 0.05 % of a
  16.667 ms frame.** Named rather than absorbed.
* **`convert` FELL, 0.056 → 0.035 ms, and this is reported unattributed.** It moved in the *opposite*
  direction to the one a newly added `Bus::publish` call inside that bucket would push, and it lands back on
  parcel 1's own documented 0.035–0.037 ms — so it is the BEFORE runs that are the outlier against the
  historical figure, not the AFTER ones. The most likely mechanism is that these are two different
  compilations (release, no LTO, 16 codegen units) and inlining around a 287 KB pixel conversion differs by
  ~20 µs. **It is not claimed as a win.**
* **`CPU TOTAL` at the median moved 2.854 / 2.946 → 2.887 / 2.937 — inside BEFORE's own between-run spread
  of 0.092 ms.** Emulation is still ~93 % of the CPU frame.
* **The tail moved slightly and it is named.** p99 is consistently ~0.15 ms higher after (17.14–17.23 →
  17.32–17.35), and the worst frame 22.52 / 22.59 → 22.75 / 23.54. Both AFTER runs carried a `sigil`
  process at 78–100 % of a core that neither BEFORE run did, which is the honest first explanation; no
  bucket shows a matching increase, `bus-pump`'s own worst is 0.013 ms, and the design absorbed it — zero
  rebases, zero steady starvations, zero producer drops, leanest ring unchanged at ~75 ms.
* **The verdict: no material regression.** Emulated frame rate, median period, starvations, drops and the
  fine trim's 2-frame rate are identical to three decimal places or exactly equal across all four runs.

`--target-fps 0` remains the **CONTROL** and not a result; the report stamps any run made with it
`GOVERNOR OFF` and nothing from it is a statement about the player.

### 5.6.2 What parcel 3 deliberately did NOT do

* ~~**No `Tab` variants**, so `LAYOUT_VERSION` is untouched at 1. Breakpoints, Watchpoints and Profiler
  tabs are the next parcel's, and `Bus::read_instruments` is the borrow they draw from.~~
  > ⚑ **SUPERSEDED by parcel 3-tabs (§5.7), and BOTH halves of the sentence changed.** The first half was
  > true of parcel 3 and is no longer true of the branch that sits on it: three `Tab` variants shipped and
  > `LAYOUT_VERSION` is **2**, and it is no longer an integer anybody bumps — it is `VOCABULARIES.len()`.
  > The second half was **wrong when written**: `Bus::read_instruments` is `(&Watchpoints, &Profiler,
  > bool)` and has never carried breakpoints, so it is the borrow *two* of the three tabs draw from. The
  > Breakpoints tab draws from `Bus::read_breakpoints`, added as its sibling. §5.7 has the reason.
* ~~**Held pads are still not merged.** `Host::set_live_pads` / `Host::held` are unwired in
  `oracle-player`, so a client's `emulator/hold` does not compose with the keyboard there (it does in
  `oracle-frontend`, via `Bus::merge_held`). §5.4 listed this among parcel 3's costs; it is a real gap and
  it is booked here rather than quietly closed. **§9.4 is its sibling and not the same defect**: that one
  is about a held set the frontend *does* apply being invisible to the human; this one is about the player
  not applying it at all, so a client's `hold` against the toolkit player does nothing whatsoever.~~
  > ⚑ **CLOSED by `HELD-PADS-PLAYER`, together with §9.4** — and the two were taken together *because*
  > they are siblings. Half 1 without half 2 makes the defect worse: today `hold` does nothing, so nothing
  > is silently wrong; applied but unshown, it is exactly the trap §9.4 describes.
  >
  > `Machine::step` now takes `[Pad; 2]`, publishes the human's pads through `Bus::set_live_pads` and
  > writes `Bus::merge_held`'s result — in `step` rather than in `Loop::iterate` (where `oracle-frontend`
  > does it) because those two `set_pad` calls are the only place the crate writes a pad, so no path can
  > grow that bypasses the merge, and because a merge in `iterate` needs a window and is therefore one no
  > test in the crate can execute. Port 1 stopped being hardcoded to `Pad::default()` in the same change.
  >
  > **The merge itself was hoisted, not copied.** §7's R1 named porting `oracle-frontend`'s model halves
  > into shared code as this parcel's opportunity, and it is taken: `Host::merge_held` in `oracle-aether`,
  > beside `held`/`set_live_pads`, is the one implementation, and the frontend's `Bus::merge_held` is now a
  > delegation to it.
  >
  > ⚑ **The `is_serving()` early return did not survive the move, and its absence is part of the fix.** It
  > was a fast path and not a semantic — `held` is `Pad::default()` until something calls `emulator/hold`,
  > and a per-button OR with a default pad is the identity, which
  > `unserved_merge_is_the_identity_the_is_serving_gate_used_to_shortcut` pins in both directions. Kept, it
  > would have been actively wrong here: this player **binds no socket**, so `is_serving()` is false
  > forever, while `Host::call` is reachable in-process (D15) and can install a held set anyway. Under the
  > gate that set would sit in the engine, be reported back by `emulator/hold`'s own `held` array, and
  > never reach the pad — a served method answering that it took effect when it did not.
* ~~**The picture does not refresh after `emulator/step`.** A step advances one *instruction*, the run
  that drew the retained frame is over, and `Host::publish_capture` is gated on `has_clients()` — which an
  unserved player never satisfies, so `Host::framebuffer()` has nothing to pull. The retained frame is the
  truthful last-drawn one; it updates on resume.~~
  > ⚑ **SUPERSEDED by `PLAYER-SERVE`, and the CONCLUSION survives while the MECHANISM does not.** The
  > picture still does not refresh after `emulator/step`. But the reason given here was wrong about this
  > crate, and re-reading it under a bound socket is what showed that.
  >
  > **What was checked.** `grep -rn "framebuffer\|present_frame\|screen_changed" crates/oracle-player`
  > returns **one hit, in a doc comment**. `oracle-player` has never called `Host::framebuffer()` and has
  > never read a `PumpReport`. So `has_clients()` was never the operative gate on the *window's* picture:
  > even with the gate wide open there was no code on this side to pull a published frame, and there still
  > is not. The gate governs what `emulator/screenshot` serves **over the socket**, which is a different
  > surface with a different consumer.
  >
  > **What actually changed.** `has_clients()` is no longer permanently false — it goes true the moment a
  > client attaches, which
  > `bus::serving::has_clients_is_false_unserved_false_unattached_and_true_once_someone_attaches` pins in
  > all three states. Two consequences, and they pull in opposite directions:
  >
  > * **A fix, for clients.** `Bus::publish` now really publishes, so `emulator/screenshot` and
  >   `emulator/state_hash {includeFramebuffer}` answer with the frame the window drew — including
  >   mid-frame palette effects a post-hoc VDP re-render cannot show. That seam had never been exercised;
  >   it is now.
  > * **A cost, for the frame budget.** It is one frame's copy per emulated frame, *conditional on a client
  >   being attached*, and §5.8.1 prices it rather than assuming it away.
  >
  > The bullet's own subject — the window's glass after a step — is unchanged and is now booked properly
  > in §5.8.2 as a gap with a real cause: the player drops the `PumpReport`.
  >
  > **⚡ SUPERSEDED by `PLAYER-PUMPREPORT` (branch `parcel/player-pumpreport`).** The glass now follows a
  > client's run. `bus::drain` takes the bus's latched frame on `screen_changed` and
  > `Machine::adopt_frame` puts it on screen, and
  > `bus::pumped::a_paused_windows_picture_follows_a_clients_run` pins it from a client on a real socket.
  > See §5.8.2.

### 5.7 Parcel 3-tabs — P4/P5/P6 themselves, on the seam

> ### ✅ **BUILT, on `parcel/panels-3-tabs`** (`fe00618` the three tabs, `017e21b` the measurement fixture).
> The panels §5.4 priced and §5.5 insisted on keeping together. `Tab` is now all eight of
> `Screen | Pacing | Registers | Memory | Objects | Breakpoints | Watchpoints | Profiler`.

**What shipped.** Three tabs, `crates/oracle-player/src/stopping.rs` as their one shared model — one module
for three tabs for the reason `memory.rs` is one module for five spaces. Every panel body is a **direct
read of the instrument the run loop itself feeds** (§4.4's hybrid, read half), and every gesture is a
`(method, params)` pair dispatched through `Bus::call` and rendered by `memory::answer_line` (§4.4's write
half), branching on `Answer::reason()` and colouring on `Answer::is_err()`. Nothing in `stopping.rs` or
`ui.rs` composes a refusal sentence: the arming rules, the caps, the handle grammar and the param types are
the handlers' own, and the panels inherit every one of them including the ones this design did not
anticipate. Two such were caught by the tests rather than by memory — `emulator/run_frames` takes `frames`
and not `n`, and it is `require_paused`.

**⚑ Breakpoints are NOT in `read_instruments`, and §5.6.2's second half was wrong to say they were.**
`Bus::read_instruments` is `(&Watchpoints, &Profiler, bool)` (`bus.rs:203`, over
`engine.rs:1698`) and has never carried a breakpoint set. The distinction is not an accident of the
signature, it is the seam's own vocabulary: **an instrument RECORDS and is lent to a run wrapped in
`Observe`; a breakpoint HALTS and is lent bare** — that is exactly how §5.6's `run_sinks(resume_pc)` hands
them over, `(Observe<&mut Watchpoints>, Observe<&mut Profiler>, BreakStop)`. So the Breakpoints tab gets a
**sibling** rather than a fourth element: `Engine::read_breakpoints` (`engine.rs:1724`) →
`Host::read_breakpoints` (`host.rs:463`) → `Bus::read_breakpoints` (`bus.rs:225`). All four reads are
`&self`, which is the only reason `read_instruments` bundles three at all — a draw pass holds every one of
them at once. The alternative, calling `emulator/breakpoint_list` on every repaint, is **route (b) on a
60 Hz path**, and §4.4's whole line is that a repaint does not pay for it.

**⚑ `Live` = Yes / Retained / Never, which is the panel-shaped answer to the trap `read_instruments`' own
doc names.** *Armed is not derivable from the rows.* It is true of all three instruments and in **three
separate mechanisms**, so no single fix covers it and no amount of correct row rendering would:

* **Profiler** — `set_profiler {enabled:false}` disarms and **retains** the sample (protocol §11.16: arming
  resets, disarming retains, reading never clears). A grid of hot routines from four minutes ago is
  pixel-identical to a grid of hot routines from now.
* **Watchpoints** — `watchpoint_clear` retires the watch and keeps its hits **on purpose** (the handler:
  *"a destructive clear would let one client erase another's evidence"*). A hit log can outlive every watch
  that could add to it.
* **Breakpoints** — `breakpoint_set_enabled {enabled:false}` carries `hits` **across the toggle** (§6 of
  the protocol: *"a client wanting a fresh count clears and re-adds"*). A disabled row reading 12,000 hits
  fired 12,000 times and will not fire again.

So a **full table says nothing about whether anything is still recording**. Each tab prints `Live` in words
before it draws a row, and `Live::Never` **refuses to draw the table at all** — the Objects tab's rule, one
instrument over: an empty grid where "never measured" belongs asserts that this ROM has no hot code.
`Retained` and `Never` are a genuinely reachable pair and not a bool, which `stopping::tests` is red for.

**`LAYOUT_VERSION` 1 → 2, and it stops being a number anybody remembers.** §6's box says *"bump it in the
same change that touches `Tab`"* — a rule with nowhere to enforce it. It is now
`VOCABULARIES.len()` (`layout.rs:56`), over an append-only table of the `Tab` vocabularies this player has
shipped, spelled in serde's own alphabet because that is the text a `DockState<Tab>` writes into the RON.
**Appending a row IS the bump**, so there is no second place to forget; what remains is changing `Tab` and
appending nothing, and one test is red for exactly that. A version-1 layout is discarded by the version
gate before `ron` sees the blob, proven on a real version-1 blob whose bytes are also shown to be readable
under the current stamp — otherwise the test would pass on an unreadable blob and prove the wrong thing.

**`--dock every-tab` — a measurement arrangement, not a layout.** `egui_dock` draws only a leaf's *active*
tab, so a bench run against `initial_dock` executes **one** of the three panels that share a pane and would
report it as the cost of adding three. `ui::every_tab_dock` puts all eight in leaves of their own: the
worst case a user could arrange, and the only arrangement in which measuring N panels measures N panels.
It neither reads nor writes a stored layout in either direction (`main.rs:910`), so a measured run can
never inherit — or overwrite — the operator's own dock.

**`--bench-arm` — a measurement fixture, refused outside a bench mode**, and the reason §5.7's numbers in
§8 item 2 are not vacuous. The three tabs are **empty until a human arms something**, so a bench run
against a fresh player draws three headlines and an add box; reporting that as *the cost of the
Breakpoints, Watchpoints and Profiler panels* is the same vacuity as a parity test comparing `[]` against
`[]`. The expensive parts of these bodies exist only once there is something to draw — sixteen breakpoint
rows, a hit log, and a `BTreeMap` of routines the panel sorts on every repaint. Every arm goes through
`Host::call` exactly as a click would, so **the fixture cannot reach a state a user could not**. Two
choices shape what its numbers mean, and both are stated at the function:

* **The breakpoints are armed DISABLED.** An enabled one at an address this ROM executes halts the player
  and there is no run left to measure. Rows draw identically (the body branches on `enabled` only to pick a
  word and a dimming), so the panel cost is unchanged — but `any_enabled()` is false, no `BreakStop` is
  attached, and **this run does not price the halt sink.** That is §5.6's number and it lives in `emulate`.
* **The watch is wide (64 KB of work RAM, writes) and the profiler is armed with `perFrame`**, which is
  what puts real rows in front of the panels. Both attach a sink, so **`emulate` moves** — that is the
  *instrument's* cost and not the panel's, and the bucket split is what keeps the two answers apart.

⚑ **Loud on unmeasurable, in both directions.** `arm_for_measurement` reads the armed state back **off the
instruments** rather than assuming its own calls landed, and `exit(2)`s with the counts if the panels would
be empty: a refused arm must never become a quietly smaller number in a table. The test does the same and
then runs eight real `iterate()` frames, asserting the hit log and the routine map are non-empty — without
that, an armed-but-never-recording instrument satisfies every other assertion in it.

`watch_wire_id` and `breakpoint_wire_id` are now `pub`. A panel reading the instrument directly still has
to name a row back to a `clear`, and a `format!("w{}")` in `ui.rs` would be a second spelling of one fact —
agreeing until the day it did not, in the one place where being wrong retires somebody else's watch.

### 5.7.1 The panel-cost measurement — and the panel that does not fit in a frame

> ### ⚑ **SUPERSEDED IN TWO PLACES by §5.7.2, and everything below is left standing as the measurement that
> found the defect.**
>
> * **The residual of two is now a residual of ONE, and it is the Watchpoints hit log.** Not by argument —
>   by two extra binaries, each carrying exactly one of the two fixes, run against the same configuration
>   in the same sitting. Fixing the hit log alone recovers the whole 14.7 ms; fixing the Profiler alone
>   recovers **nothing**. §5.7.1's own hypothesis, below, was right; its refusal to name the panel without
>   evidence was also right, and §5.7.2 is that evidence.
> * **`ui-build` in configuration C is 0.573 ms, not 15.220**, and the player holds 60 emulated fps with
>   all eight tabs visible and both instruments armed. The numbers below are the state of
>   `parcel/panels-3-tabs` at `d944286` and remain the honest record of it.
> * **A second correction to this section's own rig**, on top of the one already boxed below:
>   `pgrep -c -f "[c]argo"` **counts the watcher that runs it**, so a wait-for-quiet loop written around it
>   can never see 0. See §5.7.2.

**This is the measurement §8 item 2 booked as owed, taken for these three panels, and it does not say what
this design predicted.** §5's claim that the panel bodies are cheap was *reasoning from size*. It holds
everywhere it was tested but one: **the Watchpoints/Profiler pair, whose two bodies together cost 14.7 ms
of a 16.667 ms frame** — 167× what the four other bodies added between them. Which of that pair it is, this
measurement cannot say (below), so "one of them is wrong by two orders of magnitude" is as far as the
evidence goes and further than any single panel may be named.

**The rig is §5.6.1's, unchanged**: `--mode bench-cpu`, 75 s, `aeon/s4.debug.bin` (+ its 2,884-symbol
`.lst`), a real audio device at gain 0.0, no window and no GPU, `--expect-screen 1281x803`. **Four separate
process invocations, reported separately and never averaged.**

**Why the comparison is inside this one binary and not against the seam tip.** `--dock every-tab` and
`--bench-arm` were both added by *this* branch, so `e208a04` cannot be run with either. A BEFORE/AFTER
across the two binaries would mix **three** changes — the arrangement, the arming, and the panels — and
report their sum as "the panel cost". So all four configurations below are the *same* binary, and each
neighbouring pair differs in exactly one thing:

| | arrangement | armed | bodies actually drawn (egui_dock draws only a leaf's ACTIVE tab) |
|---|---|---|---|
| **A** | default | no | Screen, Pacing, Registers, **Breakpoints (empty)** |
| **B** | `--dock every-tab` | no | all eight; the three stopping ones **empty** |
| **C** | `--dock every-tab` | `--bench-arm` | all eight; the three stopping ones **with rows** |
| **D** | default | `--bench-arm` | Screen, Pacing, Registers, **Breakpoints (16 rows)** |

* **D − A** isolates **one** panel: only Breakpoints is drawn of the three, so arming moves that body and
  nothing else in `ui-build`.
* **C − B** is all three stopping bodies gaining their rows, at one arrangement.
* **B − A** is four *more bodies* appearing (Memory, Objects, and the two empty stopping ones).
* **D − A** in the `emulate` bucket is the **instruments'** cost, not a panel's — and it is the only pair
  where that bucket is comparable, because A/B/D run ~1.00 emulated frames per iteration and **C runs
  1.83**, so C's `emulate` covers nearly two frames and cannot be differenced against a one-frame column.

**The condition of the machine.** `pgrep -c -f "[c]argo"` was **0 before and after all four** runs below,
1-minute load average **2.0–4.0 on 16 cores**, everything else the owner's live session (Vivaldi ×3,
Discord, `kwin_wayland`, Steam helpers, and a peer lane's `oracle-frontend` at ~20 % of one core).
**Two further sittings were taken and are NOT tabled**: both ran with a peer `sigil` lane's
`cargo test --release --workspace` and its corpus jobs on the box (`pgrep` = 1 throughout, load average
9–21), which is the condition §5.6.1 discarded a run for. They are named here only because the finding
below reproduced in both — `ui-build` medians of **16.356 ms** and **16.717 ms** for configuration C — and
a result that survives a 5× swing in machine load is not a load artefact.

> ⚑ **A correction to the rig, found by running it: `pgrep -c -f "[c]argo"` = 0 is NOT "the box is
> quiet".** A peer lane's *compiled* binaries do not match that pattern. While waiting for a second clean
> sitting, `pgrep` read **0** with two `sigil` processes at **98 % of a core each** and a 1-minute load
> average of 5.1 — a machine the check calls idle and that would have cost ~2 cores of the 16. The check
> is a *cargo* check, and it catches the peer's build; it does not catch the peer's run. Every run in the
> table above also records `ps --sort=-pcpu` and `/proc/loadavg` for exactly this reason, and the four
> tabled runs show nothing but the owner's session on them. **A future measurement should gate on the
> load average as well as on `pgrep`** — this one did, which is why it has one clean sitting rather than
> three.
>
> **A second clean sitting was attempted and NOT obtained**, and that is recorded rather than papered
> over: a ten-minute wait for `pgrep` 0 *and* a 1-minute load average under 4.0 timed out with the peer
> lane still on the box. So every figure tabled above is **one clean invocation per configuration**, and
> the two deltas that live near the noise floor — `ui-build` D − A and `emulate` D − A — are qualified in
> the readings below accordingly. The 14.7 ms is not one of them: it is 60× the largest spread any
> configuration showed.

Per-iteration cost, **medians**, milliseconds:

```
part                     A         B         C         D
                    default  every-tab  every-tab   default
                     no arm     no arm    ARMED      ARMED
emulate               2.702     2.680      5.582     4.497
audio                 0.001     0.001      0.002     0.001
convert               0.036     0.035      0.071     0.053
tex-upload            0.006     0.005      0.006     0.009
ui-build              0.173     0.261     15.220     0.390     <-- the panels
tessellate            0.036     0.051      0.071     0.074
bus-pump              0.000     0.000      0.001     0.001
CPU TOTAL             2.922     2.989     20.905     5.025
period               16.664    16.664     38.672    16.669
n (iterations)         4500      4500       2113      4465
```

| | **A** | **B** | **C** | **D** |
|---|---|---|---|---|
| **emulated frames/s** | **60.038** | **60.038** | **51.581** | **60.033** |
| presented frames/s | 59.998 | 59.998 | **28.170** | 59.526 |
| emulated frames per iteration | 1.000 | 1.000 | **1.830** | 1.008 |
| frame period, median | 16.664 ms | 16.664 ms | **38.672 ms** | 16.669 ms |
| frame period, WORST | 23.689 ms | 28.040 ms | 64.375 ms | 42.573 ms |
| governor rebases | 0 | 0 | **1754** | 30 |
| **audio starvations, steady** | **0** | **0** | **1353** | **2** |
| audio producer drops | 0 | 0 | 0 | 0 |
| leanest ring | 6820 (77.3 ms) | 6526 (74.0 ms) | 2940 (33.3 ms) | 2940 (33.3 ms) |
| `ui-build` p95 / p99 | 0.306 / 0.382 | 0.366 / 0.475 | 16.873 / 23.277 | 1.600 / 2.714 |

**⚑ The finding, stated before the arithmetic: one of these panels costs 91 % of a frame budget on its own,
and the player misses real time drawing it.** Configuration C is not a stress test invented for the
occasion — it is eight tabs visible with a watch and the profiler armed, which is a layout a human can
build with the mouse and a state `--bench-arm` reaches only through `Host::call`. In it the machine runs
**51.6 emulated frames a second instead of 60**, presents **28**, rebases the governor **1754 times**, and
inserts **10.6 seconds of silence into a 75-second run**. Nothing else in the report moved: `bus-pump` is
still 0.001 ms, `convert` and `tex-upload` are unchanged, and `producer DROPS` is 0.

**The arithmetic, and what each delta may be called.**

* **`ui-build` B − A = +0.088 ms.** Four more panel bodies — Memory, Objects, and the two empty stopping
  ones — is **0.5 % of a frame**. §5's "these are cheap" is *confirmed* for the read-only panels. ⚠ One
  caveat that must travel with this number: `every_tab_dock` gives each of eight leaves ~1/8 of the window,
  so Memory's hex view and Objects' table lay out **fewer visible rows** than they would in a pane a human
  had made big. **B − A is a lower bound for those two bodies at full size, not their cost.**
* **`ui-build` D − A = +0.217 ms** — **the Breakpoints panel with sixteen rows**, isolated, in the shipped
  default arrangement. 1.3 % of a frame for the whole body, add box and all. Cheap, as designed. ⚠ This
  delta is at the edge of what one clean sitting resolves: the two discarded sittings put it at +0.100 and
  **+0.001** ms, both with an inflated A to difference against. Read it as **≲0.2 ms**, and note that every
  reading of it is under 2 % of a frame, which is the part that matters.
* **`ui-build` C − B = +14.959 ms** — the three stopping bodies gaining their rows.
* **Therefore (C − B) − (D − A) = +14.742 ms is Watchpoints + Profiler**, and **today's flags cannot split
  that pair**: no arrangement draws one of them without the other, and `--bench-arm` arms all three
  together. It is booked as a **residual of two**, not attributed to one. ⚑ *What it is not:* it is not
  the instruments recording — that is `emulate`, below — and it is not the `Live` header, which is three
  words.
* **`emulate` D − A = +1.795 ms per emulated frame** is the **instruments'** cost: a 64 KB-wide work-RAM
  write watch plus a `perFrame` profiler, feeding sinks on every access. ⚠ **It is the least stable number
  here and it must not be quoted as a figure.** The two discarded sittings put the same delta at **+0.62**
  and at **−1.82** ms — the second one *negative*, i.e. the armed run was the faster of that pair, with a
  peer `sigil` at 98 % of a core during the unarmed half — and D's own `emulate` p95 is 9.635 ms against
  A's 3.184. All that is established is that **`emulate` moves, upward, by something under two
  milliseconds a frame, and that this is the instruments' cost and not a panel's.** A 64 KB write watch is
  the expensive end of the instrument; nothing here prices a narrow one.

**The hypothesis for the 14.7 ms, kept separate from the measurement.** `ui::Panels::watchpoints` draws the
hit log as `for h in &view.hits { ui.monospace(format!(…7 fields…)) }` inside a plain
`ScrollArea::vertical().max_height(220.0)` — a **non-virtualised** list, `show` rather than `show_rows`. The
log is a ring of `EngineConfig::watch_ring_cap` = **4096** entries (`engine.rs:205`), which a 64 KB write
watch fills in well under a second, and `stopping::watches` copies all 4096 out with `hits().to_vec()` on
every repaint besides. 4096 rows × ~3.6 µs of `format!` + galley layout is ~14.7 ms, which is the size of
the residual — but *consistency is not attribution*, and the Profiler's own suspect (it sorts the whole
routine map every repaint before truncating to `TOP_ROUTINES` = 24) is untested for the same reason. **The
split, and the fix, are the next parcel's**, and the fix is named here so it is not re-derived: `show_rows`
with a fixed row height, which draws the ~10 rows a 220 px viewport can show.

> ⚑ **This hypothesis was right, and §5.7.2 is the measurement of it.** The hit log was **14.779 ms** of
> the 14.7; the Profiler's suspected sort was **0.027 ms** and never a contributor. The named fix —
> `show_rows` — is what shipped, with the row height taken from the style rather than fixed by hand. The
> paragraph is left standing because the discipline it demonstrates is the point: *consistency is not
> attribution*, and the panel was not named until a binary that fixed one and not the other said so.

**What this does NOT say.** It does not say the default player is slow: **configuration A is the shipped
layout and it is 0.173 ms of `ui-build`**, and D — the shipped layout with instruments armed and a full
Breakpoints table — holds 60.033 emulated fps with 2 steady starvations in 75 s. The regression is reachable
by clicking a tab, not by launching the program.

### 5.7.2 The retake — the residual of two was one panel, and it was the hit log

**§5.7.1 could not split its residual because no flag drew one of the pair without the other. This retake
does not split it with a flag; it splits it with the FIX.** Four release binaries were built from this
branch and run back to back in one sitting:

| binary | hit log virtualised (`show_rows`) | profiler selects before formatting |
|---|---|---|
| **BEFORE** (`d944286`) | no | no |
| **HITLOG-ONLY** | **yes** | no |
| **PROF-ONLY** | no | **yes** |
| **AFTER** (`c7c9b5a`) | **yes** | **yes** |

Each fix targets exactly one panel body, so `BEFORE − HITLOG-ONLY` is the Watchpoints hit log's cost and
`BEFORE − PROF-ONLY` is the Profiler's, with no arrangement flag needed and nothing left over to guess at.
All four are configuration **C** (`--dock every-tab --bench-arm`) so the *only* thing that differs between
them is which body was made cheap. The four binaries were confirmed to be four distinct files before the
sitting, and `AFTER` was rebuilt from a clean tree and compared byte-for-byte with the copy that was run —
a measurement in which two of the "different" builds were the same file would look exactly like a fix that
did nothing.

**The rig is §5.7.1's, unchanged**: `--mode bench-cpu`, 75 s, `aeon/s4.debug.bin` (+ its 2,884-symbol
`.lst`, auto-discovered), a real audio device at gain 0.0, no window and no GPU, `--expect-screen
1281x803`. **Separate process invocations, reported separately and never averaged.**

> ### ⚑ **A second correction to the rig, and it is worse than the first: `pgrep -c -f "[c]argo"` COUNTS THE WATCHER.**
>
> §5.7.1's box says the check is a *cargo* check that misses a peer's compiled binaries. That is true and
> it is not the whole fault. `-f` matches the **whole command line**, so a wait-for-quiet loop whose own
> text mentions `cargo` — as any loop that reports `cargo=$(…)` must — matches **itself**. Measured here:
> at 13:26 the documented check read **3** while `pgrep -c -x cargo` and `pgrep -c -x rustc` both read
> **0** and the box was genuinely idle; all three "cargo processes" were this session's own watchers. A
> gate written on it waits forever and then reports that the box was never quiet — which is very close to
> what §5.7.1's own "second clean sitting was attempted and NOT obtained" says happened.
>
> **`pgrep -x cargo` is the check.** `-x` matches the process *name*, so it cannot match a shell whose
> command line merely mentions the word. Every run below records `pgrep -c -x cargo`, `pgrep -c -x rustc`,
> `/proc/loadavg` and `ps --sort=-pcpu` on **both** sides of the run, and the `-f` reading is recorded
> beside them so the two can be compared rather than argued about. The lesson is §5.7.1's own, one turn
> further in: *a control must measure the string it names* — and a control that can match itself is not
> measuring the box, it is measuring the measurement.

**The condition of the machine, and what was discarded.** The four C runs were taken in one continuous
clean stretch: `cargo -x` **0** and `rustc -x` **0** on both sides of every one, 1-minute load average
**2.53 → 3.02** on 16 cores, nothing on the box but the owner's session (Vivaldi, Discord,
`kwin_wayland`, a peer lane's `oracle-frontend` at ~23 % of one core). **Six further runs were taken and
are DISCARDED**, named here rather than quietly dropped: a peer `sigil` lane's cargo job returned during
`B-BEFORE` and stayed, putting `B-BEFORE`, `B-AFTER`, `D-AFTER`, `A-AFTER`, `C-AFTER2` and `C-BEFORE2` at
load **4.9–8.7** with `cargo -x` 1–2. They were re-taken in a second sitting, below, behind a per-run
quiet guard. One thing the discarded pair is worth for: `C-BEFORE2`, at load 7.53, put `ui-build` at
**15.879 ms** — the defect reproduces at 2.5× the load, exactly as §5.7.1's two discarded sittings found,
and it is not a load artefact.

#### The split, at configuration C — medians, milliseconds

```
part            BEFORE   PROF-ONLY  HITLOG-ONLY   AFTER
              (neither)  (profiler)  (hit log)   (both)
emulate         5.670      5.840       2.837      2.890     <-- NOT comparable across the pair; see below
audio           0.002      0.002       0.001      0.001
convert         0.071      0.071       0.057      0.035
tex-upload      0.006      0.006       0.005      0.005
ui-build       15.379     15.436       0.600      0.573     <-- the panels
tessellate      0.070      0.071       0.052      0.052
bus-pump        0.001      0.001       0.000      0.000
CPU TOTAL      21.145     21.363       3.507      3.510
period         38.910     39.172      16.662     16.664
n (iterations)   2113       2091        4500       4500
```

| | **BEFORE** | **PROF-ONLY** | **HITLOG-ONLY** | **AFTER** |
|---|---|---|---|---|
| **emulated frames/s** | **51.577** | **51.292** | **60.050** | **60.050** |
| presented frames/s | 28.168 | 27.872 | 59.997 | 59.997 |
| emulated frames per iteration | 1.831 | 1.841 | 1.001 | 1.001 |
| frame period, median | 38.910 ms | 39.172 ms | **16.662 ms** | **16.664 ms** |
| frame period, WORST | 53.084 ms | 53.011 ms | 28.308 ms | 28.692 ms |
| worst lateness | 36.417 ms | 38.682 ms | 12.346 ms | 12.946 ms |
| **governor rebases** | **1754** | **1756** | **0** | **0** |
| **audio starvations, steady** | **1348** | **1368** | **0** | **0** |
| inserted silence | 10.599 s | 10.958 s | 0.067 s | 0.067 s |
| audio producer drops | 0 | 0 | 0 | 0 |
| leanest ring | 1176 (13.3 ms) | 1176 (13.3 ms) | 6056 (68.7 ms) | 7232 (82.0 ms) |
| `ui-build` p95 / p99 | 16.506 / 22.959 | 16.521 / 23.158 | 0.741 / 0.972 | 0.688 / 0.884 |
| load average, before → after | 2.53 → 2.82 | 3.16 → 3.02 | 3.17 → 3.16 | 3.31 → 3.17 |
| `cargo -x` / `rustc -x` | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |

**⚑ The finding: the 14.7 ms was the Watchpoints hit log, all of it, and the Profiler was never in it.**

* **`ui-build` BEFORE − HITLOG-ONLY = +14.779 ms.** Virtualising the hit log **alone** recovers the whole
  regression: 51.577 → 60.050 emulated fps, 1754 → **0** governor rebases, 1348 → **0** steady audio
  starvations, 10.6 s of inserted silence → 0.067 s. That is the Watchpoints panel's cost, isolated.
* **`ui-build` BEFORE − PROF-ONLY = −0.057 ms.** Fixing the Profiler **alone** recovers **nothing** — the
  sign is negative, which is to say the difference is noise, and the run is still 51.292 fps with 1756
  rebases and 1368 starvations. **The Profiler was not a contributor and the ranking it does was never the
  cost.**
* **The Profiler's own cost, isolated the other way round** — HITLOG-ONLY − AFTER = **0.027 ms**, with the
  expensive neighbour already out of the way. **0.16 % of a frame.** §5's reasoning-from-size claim is
  *confirmed* for the Profiler body. ⚠ It is confirmed **at this ROM's routine count**, which this report
  does not print and which is therefore not stated here; the body is `O(n)` in the sample either way now,
  and it was the `O(n)` formatting rather than the sort that the fix removed.
* **The two add up.** 14.779 + (−0.057) = 14.722 against BEFORE − AFTER = **14.806 ms** measured directly:
  the parts agree with the whole to 0.084 ms, which is what a genuine partition looks like and what a
  mis-attribution would not survive.
* **`emulate` must NOT be differenced across this table.** BEFORE and PROF-ONLY run **1.83 emulated frames
  per iteration** and HITLOG-ONLY and AFTER run **1.00**, so the BEFORE column's `emulate` covers nearly
  two frames. Its apparent halving is the iteration getting shorter, not the emulator getting faster —
  §5.7.1's own caveat about configuration C, and it applies within this table as well as across it.

**What the fixes were, and that they changed nothing a human sees.** `crates/oracle-player/src/ui.rs`
draws the hit log with `ScrollArea::show_rows` rather than `show` — the rows are uniform (every one is a
single `ui.monospace` line), the height comes from `ui.text_style_height(&TextStyle::Monospace)` rather
than a typed number and is passed **sans spacing** because `show_rows` adds `item_spacing.y` itself
(egui 0.36.1 `scroll_area.rs:991`), and `stick_to_bottom` survives because `show_rows` sets the content
height for the whole virtual list. `stopping::top_routines` ranks the bare `(addr, counts)` pairs with one
`select_nth_unstable_by` and formats only the `TOP_ROUTINES` that survive; the answer is provably the old
one because the row order is a **total** order over a `BTreeMap` keyed by address, and
`the_top_selection_matches_a_full_sort_even_with_ties` re-derives the old algorithm and compares against
it on a fixture whose 32-row tie group straddles the 24-row cut.

#### The four configurations, re-taken — so `C − B` is the same subtraction §5.7.1 booked the defect with

A **second sitting**, behind a per-run quiet guard, because the first lost B/D/A to the returning peer. Same
binary throughout (`AFTER`) except `B-BEFORE`, which is there to check that this branch's B reproduces
§5.7.1's B rather than being compared against a number from another day.

```
part                     A         B         C         D    |  B-BEFORE
                    default  every-tab  every-tab   default  |  every-tab
                     no arm     no arm    ARMED      ARMED   |    no arm
emulate               2.754     2.733     2.890     2.907    |    2.738
audio                 0.001     0.001     0.001     0.001    |    0.001
convert               0.035     0.035     0.035     0.035    |    0.035
tex-upload            0.005     0.005     0.005     0.006    |    0.005
ui-build              0.167     0.250     0.573     0.225    |    0.260   <-- the panels
tessellate            0.035     0.049     0.052     0.041    |    0.050
bus-pump              0.000     0.000     0.000     0.000    |    0.000
CPU TOTAL             2.969     3.031     3.510     3.182    |    3.061
period               16.666    16.666    16.664    16.665    |   16.664
n (iterations)         4500      4500      4500      4499    |    4498
```

| | **A** | **B** | **C** | **D** | | **C, §5.7.1** |
|---|---|---|---|---|---|---|
| **emulated frames/s** | **60.051** | **60.051** | **60.050** | **60.037** | | 51.581 |
| presented frames/s | 59.998 | 59.997 | **59.997** | 59.984 | | 28.170 |
| emulated frames per iteration | 1.001 | 1.001 | **1.001** | 1.001 | | 1.830 |
| frame period, median | 16.666 ms | 16.666 ms | **16.664 ms** | 16.665 ms | | 38.672 ms |
| frame period, WORST | 23.630 ms | 28.153 ms | 28.692 ms | 32.082 ms | | 64.375 ms |
| **governor rebases** | **0** | **0** | **0** | **1** | | 1754 |
| **audio starvations, steady** | **0** | **0** | **0** | **0** | | 1353 |
| audio producer drops | 0 | 0 | 0 | 0 | | 0 |
| leanest ring | 7584 (86.0 ms) | 6232 (70.7 ms) | 7232 (82.0 ms) | 5350 (60.7 ms) | | 2940 (33.3 ms) |
| `ui-build` p95 / p99 | 0.249 / 0.350 | 0.323 / 0.454 | 0.688 / 0.884 | 0.499 / 0.997 | | 16.873 / 23.277 |
| load, before → after | 3.09 → 3.75 | 2.80 → 2.14 | 3.31 → 3.17 | 2.14 → 4.83 | | 2.0–4.0 |
| `cargo -x` / `rustc -x` | 0/0 both sides | 0/0 both sides | 0/0 both sides | 0/0 both sides | | — |

**⚑ The player holds real time with every tab open and both instruments armed.** Configuration C — the one
that was 51.6 emulated fps, 28 presented, 1754 rebases and 1353 steady starvations — is **60.050 emulated
fps, 59.997 presented, 0 rebases and 0 steady starvations**, with the same eight bodies drawing the same
rows. `ui-build` is **0.573 ms**, 3.4 % of a frame, against 15.220 ms and 91 %.

**The arithmetic, recomputed against the same subtractions §5.7.1 used.**

* **`ui-build` C − B = +0.323 ms**, against §5.7.1's **+14.959 ms**. The three stopping bodies gaining
  their rows is **1.9 % of a frame**, down from 90 %.
* **`ui-build` B − A = +0.083 ms**, against §5.7.1's +0.088. Four more bodies; unchanged by this parcel, as
  it must be — neither fix touches Memory or Objects — and reproducing to 0.005 ms is a check on the
  sitting, not a finding. §5.7.1's caveat still travels with it: `every_tab_dock` gives each leaf ~1/8 of
  the window, so this is a **lower bound** for Memory and Objects at full size.
* **`ui-build` D − A = +0.058 ms**, against §5.7.1's +0.217 (which that section already qualified as
  **≲0.2 ms**, its two discarded sittings putting it at +0.100 and +0.001). This reading is inside that
  band and adds no precision to it. ⚠ It also should not be read as a change: neither fix touches the
  Breakpoints body, so **D and A are the same code before and after**, and the movement here is the noise
  floor of a single sitting, which is what §5.7.1 said it was.
* **Therefore `(C − B) − (D − A)` = +0.265 ms is Watchpoints + Profiler**, against §5.7.1's **+14.742 ms**
  — **a 56× reduction**, and 1.6 % of a frame rather than 88 %.
* **The two independent routes to the same number agree to 0.01 ms.** The residual route says the pair cost
  `(C−B) − (D−A)` = (15.119 − 0.058) = **+15.061 ms** before the fixes, using this sitting's own
  `B-BEFORE` = 0.260. The fix-isolation route says the fixes removed **14.806 ms** and left **0.265 ms**
  behind, i.e. **15.071 ms** before. Two different subtractions over two different sittings, 0.010 ms
  apart. That agreement is the reason the attribution above is stated as a measurement and not as a story
  that fits.

**The condition of the machine for this second sitting**, per run, both sides: A, B, C and D all read
`cargo -x` **0** and `rustc -x` **0** before *and* after. Load averages are tabled above. Two tails are
worth naming rather than smoothing: **`B-BEFORE` finished with a peer `rustc ×2` arriving in its last
seconds** — its median 0.260 is the figure used and it reproduces §5.7.1's B (0.261) to 0.001 ms, but its
`emulate` p99 of 6.715 ms against B's 3.197 is that peer and not the player. **`D-AFTER` ended at load
4.83** with a peer `sigil` binary at 96 % of a core, which is where its `emulate` p95 of 5.309 ms and its
single governor rebase come from. Neither tail touches a median this section leans on.

⚠ **One respect in which the drawn output is not literally identical, stated rather than glossed.** A
vertical `ScrollArea` has `auto_shrink.x = true` by default (`scroll_area.rs:397`, applied at `:1186`), so
its width follows its content — and its content is now the ~10 visible rows instead of all 4096. Row
widths vary by a few monospace characters (`{:#X}` on the hit value, `{:?}` on op and size), so the hit
log's **vertical scrollbar can now sit a few characters further left or right as the log is scrolled**,
where before it was pinned by the widest row in the whole ring. The rows themselves — text, order, count,
scroll position, stick-to-bottom — are unchanged. The alternative, `auto_shrink([false, true])`, would
pin the scrollbar to the panel's right edge, which is a *larger* departure from what ships today, so it
was not taken.

---

### 5.8 `PLAYER-SERVE` — the window binds a socket

> ### ✅ **BUILT, on `parcel/player-serve`.**

**The defect.** Everything above this section built a debug window that no external tool could reach. The
player owned a `Host` and never called `Host::serve`, which `crates/oracle-player/src/bus.rs`'s module doc
stated outright: *"No `Host::serve`. No socket is bound, no filesystem entry is created and no thread is
started."* So `emulator/hold`, every panel gesture and the whole method registry worked **in-process only**,
and another lane's tool, a script or the MCP shim got nothing at all.

**What shipped.** `--aether`, `--socket PATH` and `ORACLE_AETHER=1`, carrying `oracle-frontend`'s
three-state `Option<Option<PathBuf>>` verbatim — `None` binds nothing, `Some(None)` binds the resolved
default, `Some(Some(p))` binds `p`. The parser folds all three switches at one place, so `--socket P
--aether` still binds `P` in either order. Nothing about the default launch changed: no flag, no socket, no
thread.

#### The socket is for EXTERNAL clients only, and the GUI is not a client of itself

Written at the seam rather than left to be inferred, because "unify the two paths" is the tidying a later
reader will reach for. `empyrean:contract/protocol.md` **D15**: *"An in-process GUI is a consumer of the
same registry, not a second server… it reads the method registry directly, in-process; it does not open a
socket to itself."* The panels keep reading the shared derivation directly and every gesture keeps going
through `Host::call`. Routing them through the socket would add a `serde_json` round trip and a thread hop
per click, and would have to land in `Host::pump` — **the drain that runs once per iteration** — so a click
would cost a frame before it was even dispatched. That is the option this repo already considered and
rejected, and it is the worse of the two.

#### The default path — the decision, and what a collision does

**The default stays the contract's own resolver** (`$ORACLE_SOCKET` → `$EXODUS_SOCKET` →
`$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`, §7.1), which on this box is
`/run/user/1000/oracle.sock` — the path the owner's `oracle-frontend` has historically held.

That collision is real, and binding elsewhere by default would have been the *worse* answer: every lane's
client resolver commits to that path, so a player with a private default would bind a socket and still leave
"nothing can attach to this window" true in practice. The flag would do nothing a tool could use.

So the collision is made **loud** instead of avoided, and `Server::bind` already draws exactly the right
line — it was not changed:

* It **connects first**. Something answers ⇒ `AddrInUse`, *"another Aether server is already live on
  {path}"*, and this player prints `aether: NOT serving — cannot bind the socket (…)` and carries on
  unserved. The incumbent is untouched.
* Nothing answers ⇒ the entry is a corpse, and it is unlinked **only if it is actually a socket** (a typo
  naming a ROM or a listing is refused, not deleted).
* There is **no silent fallback to a second path**, and there is no unlink that could remove a live
  server's entry.

Pinned by `bus::serving::a_live_server_on_the_path_is_refused_and_never_stolen`, which asserts both halves:
the second window fails to bind **and** the first is still serving and still connectable. A "fix" that
unlinked the incumbent to make room passes the first half and fails the second — and that mutation was run,
on disk, and does fail there.

**The collision is not hypothetical, and it was live while this parcel was written.** Observed, without
touching the process:

```
$ stat -c '%F %a %n' /run/user/1000/oracle.sock
socket 600 /run/user/1000/oracle.sock          # created 12:20 today
$ ps -eo pcpu,comm --sort=-pcpu | grep oracle-frontend
 22.3 oracle-frontend
```

So a `--aether` launch of the player on this box, right now, would print `aether: NOT serving — cannot bind
the socket (another Aether server is already live on /run/user/1000/oracle.sock)` and carry on unserved,
leaving his window untouched. That was **not** demonstrated by dialling his socket: `Server::bind`'s
liveness probe is a connect, and connecting to an emulator this lane did not start is exactly the thing the
standing invariants forbid. The behaviour is pinned by the test instead, against a server this process owns.

A bind failure is never fatal. Someone who launched a game to play it is not stopped by a socket.

#### Saying so out loud — and improving on the frontend's shape

`oracle-frontend`'s `bus.rs:84` prints *"aether: not serving — no --aether given, so nothing can attach to
this window (pass --aether, or --socket PATH, or set ORACLE_AETHER=1)"*, and the comment at `:101` records
what earned it: the code printed **nothing at all** when not serving, so a launch without the flag emitted
no line about Aether anywhere — and **an absence is not a statement**. The measured cost was the owner
launching twice in one evening and going to a window that could not be attached to. That behaviour is
reproduced here, and improved on twice:

1. **The line is a value, not a side effect.** `ServeOutcome::sentence()` returns it; `main` prints it.
   The frontend's own comment says why it had to defend the silent arm with a `match` instead of a test —
   *"a unit test cannot read `println!`"*. The `match` is kept (deleting the silent arm is still a compile
   error) **and** all three sentences are now covered by assertions, including that they are pairwise
   different. The `println!` in `main` is unconditional, in one call site, with no per-case arm to delete.
2. **The window says it too.** A new `aether` row on the status strip, built from the same sentence — so
   the terminal and the window cannot describe one window's bus differently — reporting the path and
   whether a client is attached. **It never hides itself**, unlike the held-pads row beside it, and the
   asymmetry is the decision: the held row draws nothing when nothing is held because a permanent
   `(none)` is a line readers learn to skip, whereas the state *this* row most needs to report is the
   quiet one, which an only-when-interesting row would render as blank space — reproducing the original
   defect inside the window.

#### Every `is_serving()`, re-read

The previous parcel was caught by a guard that was correct in one host and vacuous in another —
`oracle-frontend`'s `is_serving()` early return in `merge_held`, inert in a player that never served. This
parcel makes `is_serving()` meaningful in `oracle-player` for the first time, so every use was re-read:

| Site | Before | After |
|---|---|---|
| `oracle-aether/src/host.rs:265` — the definition (`accept.is_some()`) | — | unchanged |
| `oracle-aether` `Host::merge_held` (host.rs:330) — the **absence** of a gate, argued in the doc | The argument leaned on *"this player binds no socket, so `is_serving()` is false forever"*. Load-bearing then. | Still correct, and **stronger**: a socket client's `hold` now reaches the same held set through the same engine, so the deleted gate would drop two kinds of caller instead of one. No code change. |
| `oracle-aether/src/host.rs:719, 924, 956` — its own tests | assert `!is_serving()` on a bare `Host` | unchanged; a bare `Host` still binds nothing |
| `oracle-frontend/src/bus.rs:137` + `main.rs:2462, 2487` — the status field and the screen-text snapshot gate | the frontend's own, already meaningful there | untouched by this parcel |
| **`oracle-player` — non-test uses** | **none. Zero. It appeared only in prose.** | **still none, deliberately** |

That last row is the finding. `oracle-player` gates nothing on `is_serving()` and should not: `ServeOutcome`
carries *why* (`NotAsked` and `Failed` are different facts a `bool` collapses), and a second weaker spelling
of one state is the drift R2 exists to prevent. `Bus::is_serving`, `Bus::serve_outcome` and
`Bus::socket_path` are therefore **`#[cfg(test)]`** — they exist so a test can compare the `Host`'s own view
against our stored copy, and a future gate on one of them does not compile until somebody deliberately
removes the attribute.

### 5.8.1 The pacing retake — because serving deletes §5.6.1's premise

§5.6.1 measured `bus-pump` at 0.000 ms median **with an explicit reason**: *"an unserved player queues
nothing, so the drain is a `try_recv` on an empty channel."* A bound socket can queue, and an attached
client makes `Bus::publish` copy a frame. Both premises die here, so the measurement was retaken rather
than inherited.

**The rig is §5.6.1's and §5.7.1's, unchanged**: `--mode bench-cpu`, 75 s, `aeon/s4.debug.bin` (+ its
2,884-symbol `.lst`), a real audio device at gain 0.0, no window and no GPU, `WAYLAND_DISPLAY` and
`XDG_SESSION_TYPE` unset. **Twelve separate process invocations, reported separately and never averaged**,
from one release binary built at this branch's tip. `bench-window` was not run and is forbidden for this
lane; the owner's screen was not touched.

**Four configurations, because "served" is two different questions.** A brief that measured only
served-with-nobody-attached would let that stand in for "served", and the interesting gate —
`has_clients()` — is shut in exactly that case.

| | what it is |
|---|---|
| **A** | **unserved** — no flag, today's behaviour. **The control.** |
| **B** | `--socket /tmp/orb/s`, **nobody attached**. The socket exists; `has_clients()` is false. |
| **C** | served, **one idle client** holding the connection and issuing nothing. `has_clients()` true. |
| **D** | served, **one client polling `emulator/status` at 10 Hz** — 719 requests inside the 72 s it was up. |

**The condition of the machine.** `pgrep -x cargo` and `pgrep -x rustc` were both **0** before every one of
the twelve (checked with the `-x` forms; `pgrep -c -f "[c]argo"` counts its own watcher and reads 3 on an
idle box). Everything else was the owner's live session throughout — Vivaldi, Discord, `kwin_wayland`,
stremio, `next-server`, and **his own `oracle-frontend` at ~22 % of a core** — plus a peer lane's `sigil` at
78–100 % and a `python3` at ~88 % for much of sittings 1 and 2. Load average **1.5–9.1 on 16 cores**,
recorded per run below. **Sittings 1 and 2 are contaminated at the tail and sitting 3 is not**; that is
argued rather than asserted, below.

Per-invocation headline, and per-iteration cost at the **median**, milliseconds:

| | **A1** | **B1** | **C1** | **D1** | **A2** | **B2** | **C2** | **D2** | **A3** | **B3** | **C3** | **D3** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **emulated frames/s** | 60.024 | 59.926 | 59.631 | 60.044 | 60.042 | 59.956 | 60.051 | 60.040 | **60.048** | **60.039** | **60.038** | **60.038** |
| presented frames/s | 59.891 | 59.526 | 58.671 | 59.911 | 59.962 | 59.156 | 59.918 | 59.973 | 59.981 | 59.865 | 59.998 | 59.998 |
| **period, median** | 16.666 | 16.667 | 16.666 | 16.666 | 16.666 | 16.665 | 16.665 | 16.664 | 16.666 | 16.665 | 16.665 | 16.666 |
| period, WORST | 39.4 | 89.9 | 85.9 | 30.9 | 36.8 | 72.4 | 31.3 | 43.7 | 26.9 | 35.7 | 28.9 | 23.0 |
| governor rebases | 6 | 20 | 59 | 5 | 2 | 47 | 5 | 1 | 1 | 9 | **0** | **0** |
| **audio starvations, steady** | 1 | 11 | 36 | 0 | 0 | 14 | 0 | 0 | **0** | **0** | **0** | **0** |
| **audio producer drops** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** |
| load avg at run's end | 3.96 | 9.11 | 7.18 | 2.89 | 1.50 | 5.57 | 5.44 | 3.30 | 7.37 | 6.21 | 4.15 | 3.17 |
| | | | | | | | | | | | | |
| emulate | 2.711 | 2.749 | 2.763 | 2.767 | 2.743 | 2.730 | 2.736 | 2.745 | 2.760 | 2.739 | 2.715 | 2.699 |
| audio | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 | 0.001 |
| **convert** | **0.035** | **0.036** | **0.092** | **0.092** | **0.035** | **0.036** | **0.092** | **0.092** | **0.036** | **0.036** | **0.093** | **0.092** |
| tex-upload | 0.005 | 0.006 | 0.005 | 0.005 | 0.005 | 0.006 | 0.005 | 0.005 | 0.006 | 0.006 | 0.005 | 0.005 |
| ui-build | 0.160 | 0.178 | 0.165 | 0.162 | 0.158 | 0.174 | 0.171 | 0.165 | 0.186 | 0.185 | 0.166 | 0.158 |
| tessellate | 0.033 | 0.038 | 0.034 | 0.033 | 0.032 | 0.036 | 0.036 | 0.034 | 0.039 | 0.039 | 0.034 | 0.033 |
| **bus-pump** | **0.000** | **0.001** | **0.000** | **0.000** | **0.000** | **0.000** | **0.000** | **0.000** | **0.001** | **0.001** | **0.000** | **0.000** |
| CPU TOTAL | 2.922 | 2.982 | 3.042 | 3.031 | 2.951 | 2.955 | 3.010 | 3.014 | 2.996 | 2.969 | 2.986 | 2.956 |

`bus-pump`, the other four statistics, milliseconds:

| | A1 | B1 | C1 | D1 | A2 | B2 | C2 | D2 | A3 | B3 | C3 | D3 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| p95 | 0.001 | 0.002 | 0.002 | 0.001 | 0.001 | 0.002 | 0.001 | 0.001 | 0.002 | 0.002 | 0.001 | 0.001 |
| p99 | 0.001 | 0.002 | 0.002 | 0.001 | 0.001 | 0.002 | 0.001 | 0.001 | 0.002 | 0.002 | 0.001 | 0.001 |
| worst | 0.005 | 0.021 | 0.084 | 0.027 | 0.018 | 0.014 | 0.016 | 0.005 | 0.022 | 0.071 | 0.169 | 0.005 |

#### Reading it

**1. The one real, reproducible cost is `convert`, and it is `Bus::publish` becoming true.** The median
goes **0.035–0.036 ms with no client → 0.092–0.093 ms with one**, in **all six** client-attached runs and
in none of the six without. `bus.publish(&self.cap)` sits inside the `convert` bucket in
`Machine::step`, and `Host::publish_capture` is gated on `has_clients()` — so this is exactly the gate
§5.6.2 said an unserved player could never satisfy, opening, priced. **+0.057 ms is 0.34 % of a 16.667 ms
frame.** It is charged only while somebody is attached; configuration B pays nothing.

It reproduces to three decimal places across three sittings under three different loads, which is what
makes it a finding rather than a sitting. Contrast §5.6.1's `convert` movement, which that section
reported unattributed and explicitly did **not** claim: this one has a named mechanism, a gate that
switches it, and a control on the other side of the switch.

**2. `bus-pump` did NOT become expensive, and that contradicts the hypothesis this retake was ordered
on.** The premise §5.6.1 rested on — *"an unserved player queues nothing, so the drain is a `try_recv` on
an empty channel"* — is genuinely dead: configurations C and D queue. The **conclusion** survives anyway.
`bus-pump` is 0.000–0.001 ms median and 0.001–0.002 ms at p99 in **every one of the twelve**, including
the ones taking 719 requests, and its worst single iteration across all twelve is **0.169 ms** against
`HostConfig::pump_budget`'s 4 ms ceiling. The reasoning is now: the drain is bounded and `try_recv`-only,
and a 10 Hz client puts work in roughly one iteration in six, where `emulator/status` is a handful of
field reads. **Reported as measured, not reconciled**: the retake was owed because the *reason* had
expired, and the honest outcome is that the number had a second reason.

**3. Nothing else moved.** Emulated frame rate, median period, producer drops and the 0-frame count are
identical or within a thousandth across all twelve. `ui-build`'s 0.158–0.186 ms spread does not track
configuration (A3 and B3 are the two highest, and B3 has no client at all); it tracks load.

**4. The tails in sittings 1 and 2 are contamination, and the evidence is that they do not follow the
configuration.** B — the *lightest* served configuration, with no client at all — has the worst tails in
both of those sittings (20 and 47 rebases, 11 and 14 steady starvations, an 89.9 ms worst period), while
D — the *heaviest* — is clean in both. That ordering is impossible as a configuration effect and exactly
what a busy box produces: the load average at the end of B1 was 9.11 and at the end of D1 was 2.89, and
`emulate`, which this parcel does not touch at all, moves with it (p99 10.2 ms in B1 against 4.8 ms in
A1). **Sitting 3 is the comparable one** — all four configurations back to back at load 3.2–7.4, with
**zero** steady starvations, zero drops, and 0–9 rebases across the four. The `convert` finding is the
same there as in the two noisy sittings, which is the point of having taken three.

**The verdict: no material regression, and one named, gated, sub-percent cost.**

`--target-fps 0` remains the **CONTROL** and was not used here; nothing above was taken with the governor
off.

#### ⚑ UNMEASURED, named rather than implied

* **A heavy client.** D polls `emulator/status` — a handful of field reads — at 10 Hz. A client running
  `emulator/screenshot` (a whole framebuffer, serialised), `emulator/read_memory` over large ranges, or
  `emulator/run_frames` against a paused window is a different load entirely, and **`bus-pump`'s
  0.000 ms median is not a statement about it.** `HostConfig::pump_budget`'s 4 ms ceiling exists for that
  case and was never approached here.
* **More than one client.** Every served run had exactly one connection. `publish_capture` is one copy
  regardless, but the accept path and the event fan-out are not exercised at *n* > 1.
* **A client that moves the machine.** No run issued `run_frames`, `restore` or `reload_rom`, so nothing
  here prices the `PumpReport` flags §5.8.2 books as dropped — and, being dropped, they have no cost to
  price yet either way.
* **The window.** `bench-cpu` has no window and no GPU by construction. Serving adds no draw work, and
  the one new UI element (the `aether` status row) is one `ui.monospace` line inside `ui-build`, which
  did not move outside its own between-run spread.


### 5.8.2 What `PLAYER-SERVE` deliberately did NOT do — two gaps, booked

Both are reachable *because* the socket now exists. Neither is closed here, and both are written down
rather than left to be discovered, because a gap that is booked is a decision and a gap that is silent is a
defect.

> ### ✅ **The first bullet is CLOSED by `PLAYER-PUMPREPORT` (branch `parcel/player-pumpreport`).**
>
> `Bus::mirror_pause` returns the report (`#[must_use]`) and the whole reaction lives in one new
> `bus::drain` that the loop and its tests both call — one function rather than a pump with a reaction
> beside it, because the second shape is precisely what let the reaction go missing here. **Per field, with
> the two that correctly do nothing said out loud, since an unhandled field and a deliberately-inapplicable
> one look identical in code:**
>
> | field | what this window does | vs `oracle-frontend` |
> |---|---|---|
> | `calls` | nothing — no derived state is a function of how many commands went past | same (ignored) |
> | `deferred` | nothing — the remainder is taken next iteration, nothing is lost | same (ignored) |
> | `timeline_moved()` | drop the scanline capture; rebuild the audio ring and its clock | same |
> | `frames_advanced()` | **nothing, deliberately** | frontend adds it to `draws` |
> | `screen_changed` | adopt `Host::framebuffer` onto the glass | same in effect, new code here |
> | `rom_changed` | re-derive the two pieces of cartridge-derived cache — the symbol listing **and the ROM path** — from the engine; also drive the timeline repair | frontend re-derives a save-state fingerprint instead |
>
> **The answer to “is the right fix to do what the frontend does?” is no, in both directions.** One of the
> frontend's reactions is meaningless here and one thing it never needed is required:
>
> * **`frames_advanced()` must NOT be carried here.** The frontend's `draws` is the only frame coordinate
>   its title bar shows, so it adds the bus's frames to it. This window shows two rows and they are
>   already right: `frame (emulated)` is `mclk / MCLK_PER_FRAME` read live off the machine
>   (`ui::StatusStrip::of`), so a client-driven run is on the glass with no help at all — and the other row
>   is labelled *“frames run (player)”*, which is what `Machine::frames` means and what `report.rs` divides
>   by elapsed seconds to state this loop's throughput. Adding a client's frames would falsify a label and
>   corrupt a measurement to repair a coordinate that was never wrong.
> * **The cartridge-derived cache is required here and the frontend has no equivalent.** This player keeps
>   a clone of the listing it handed to `Bus::new` **and the ROM path it was launched with** (the status
>   strip's `rom` row), and `emulator/reload_rom` moves both — it can *drop* the engine's listing on the D7
>   binding check, and it always replaces the path. Without a re-derivation the strip and the panels go on naming addresses out of a listing
>   the engine has discarded while `emulator/lookup_symbol` says there is none — one machine, two answers.
>   This is `rom_changed`'s **only** unique job in this crate: everything else it asks for,
>   `timeline_moved()` asks for on the same drain.
>
> **Two measurements worth keeping**, both pinned as assertions rather than left in prose:
>
> 1. **A reset raises `rom_changed` *and* moves the clock**, because `System::reset` rebuilds the `System`
>    and its scheduler. So `drain`'s `|| report.rom_changed` is redundant for the timeline repair in every
>    case this crate can reach today. It is written anyway — the `PumpReport` doc asks for it, and the case
>    where the two flags separate (a reset at `mclk == 0`; any future producer that replaces the machine
>    without moving its clock) is not exotic. The test asserts the redundancy, so the day it stops holding
>    the suite says so rather than quietly changing meaning.
> 2. **The audio half is the severe one and it needed a seam the crate did not have.**
>    `AudioSink::on_step_boundary` renders only on a *strictly advancing* frame index, so a reset — which
>    puts that index back to 0 — left the window silent until the machine climbed back past where it was: a
>    minute in, a minute of silence, with nothing on screen saying why. `Device` had dropped the callback's
>    flush flag into the closure on parcel 1's stated grounds that no caller existed; one exists now, and
>    `Device::resync` is it. **Named limit:** `Device` holds a live `cpal::Stream` and cannot be built
>    without a sound card, so the ring flush and the sink rebuild are witnessed in-crate only through the
>    branch being taken; the two functions they call are covered where they live. `oracle-frontend` has the
>    identical limit on the identical state.
>
> **Also learned on the way past:** `Host::call` is not a drain, so a command sent in-process can never
> appear in a `PumpReport`. Every test for this is therefore driven by a real client over a private socket,
> which is the only route that reaches the code at all. Today nothing in `ui.rs` calls a machine-replacing
> method, so no in-process gesture can go unreported; a future transport button for reset or reload would
> be outside this signal and needs its own answer.

* ~~**The `PumpReport` is dropped, and now it can carry something.**~~ `Bus::mirror_pause` calls `Host::pump`
  and discards its report. Until this parcel that was *sound*, and the reason is the reason it no longer
  is: `timeline_moved`, `screen_changed` and `rom_changed` all describe a **socket client** moving the
  machine behind the loop's back, and there was no socket. Now there is. So after a client's
  `emulator/run_frames`, `emulator/restore` or `emulator/reload_rom`, this player does not resynchronise
  its audio ring, its frame counter or its scanline capture, and does not present the frame the run drew.
  `oracle-frontend` reacts to exactly these flags; `oracle-player` reads `Host::framebuffer()` **nowhere**
  — `grep -rn "framebuffer\|present_frame\|screen_changed" crates/oracle-player` returns one hit, in a doc
  comment. **This, and not `has_clients()`, is why the picture does not refresh after `emulator/step`**
  (see the correction in §5.6.2). Closing it is a behaviour change to the run loop with its own pacing
  cost — a parcel, not a line.
* **`emulator/screen_text` is unwired here.** `oracle-frontend` publishes its title bar and status line
  through `Host::set_screen_text` (§11.29, CR-H); `oracle-player` has no such call, so a client that
  attaches to this window and asks what a human can read on it gets the engine's empty answer. Before this
  parcel no client could ask; now one can. Named, not fixed.

---

## 6. Layout persistence

> ### ✅ **BUILT AND ON, as of the `parcel/layout-persist` branch.**
>
> The rest of this section is the design as written before it was built, kept because its argument is the
> one that was implemented. **What actually shipped, and where it differs:**
>
> * **It is on.** `crates/oracle-player/src/layout.rs` is the whole of it: `save(storage, dock)` and
>   `load(storage) -> (DockState<Tab>, Outcome)`. `main.rs` wires them into `eframe::App::save` and the
>   `run_native` creation closure. `ui::initial_dock()` is now the **fallback**, not the layout.
> * ~~**The version integer is `LAYOUT_VERSION = 1`**~~, and it lives in its **own storage key**
>   (`oracle_player_dock_layout_version`) *beside* the blob (`oracle_player_dock_layout`) rather than
>   inside it — so a layout from another `Tab` vocabulary is refused before a deserializer sees it.
>   ~~Bump it in the same change that touches `Tab`.~~
>   > ⚑ **SUPERSEDED by §5.7.** It is **2**, and it is no longer an integer at all: `LAYOUT_VERSION =
>   > VOCABULARIES.len()` over an append-only table of the `Tab` vocabularies shipped so far. The struck
>   > instruction was a rule with nowhere to enforce it — appending the row **is** the bump now. The
>   > storage-key half of this bullet still stands exactly as written, and version 1 blobs are discarded
>   > by it.
> * **Discard wholesale, never migrate**, as designed below. Version mismatch, missing or junk version,
>   corrupt bytes, truncation, an unknown tab name: one fallback path, the default layout, a line on
>   stderr, nothing in the UI. No `Tab::Unknown(String)`.
> * **eframe's own storage, not a hand-rolled config file** — the framework owns the per-OS path, and
>   `persistence` also gets the window's geometry remembered. One correction to the mechanics: eframe
>   0.36.1 exposes `CreationContext::storage` as a public **field**, not the `storage()` accessor that
>   `Frame` carries.
> * ⚠ **The saved layout is RON and cannot be JSON — a fact this section did not anticipate.** Every dock
>   node carries an `egui::Rect`, and one that has not been laid out yet holds `Rect::NOTHING`, i.e.
>   `±f32::INFINITY` (`emath-0.36.1/src/rect.rs:55`). `serde_json` writes non-finite floats as `null` and
>   then refuses to read one back as an `f32`, so a JSON layout file would have been lossy from the very
>   first save — before the first repaint filled the rects in. RON spells them `inf`. The test
>   `the_default_layout_holds_non_finite_rects_which_is_why_this_is_ron` fails if that ever stops being
>   true, rather than a comment claiming it. This means §9.2's "persisting by hand with `serde_json` to a
>   config path avoids `ron`" **is not an available alternative** for this type.
> * ⚠ **`bench-window` neither persists nor restores.** eframe's restore path is not symmetric with its
>   save path: `persist_window` gates *writing* the window geometry
>   (`eframe-0.36.1/src/native/epi_integration.rs:412`) but `load_window_settings` on the way in is not
>   gated by it at all (`wgpu_integration.rs:1105`). A bench run sharing the player's storage would
>   silently inherit whatever size the operator last dragged the window to and ignore `--size`, which the
>   `--expect-screen` guard cannot catch because it checks the *monitor*, not the window. The measured
>   modes get a per-process `persistence_path` scratch file, removed on exit, plus
>   `persist_egui_memory() == false`.
> * **§9.2 was right and this section was wrong about the cost**: two feature flags, not one. Confirmed by
>   building it. The lock-file delta is `ron 0.12.2`, `enumn`, `typeid` — three packages — and
>   `cargo tree -p oracle-core` / `-p oracle-frontend` still match `egui|eframe|wgpu|winit` **zero** times
>   and carry no `ron`.
> * **Nine tests** in `layout::tests`, driven through a `BTreeMap`-backed `eframe::Storage` so they run the
>   shipped seam. Each was proven red-first by a mutation applied to disk and restored from a committed
>   baseline; the sharpest result is that with `save` writing the default layout *and* the "non-default"
>   fixture returning the default, **eight of the nine still pass** — only
>   `the_layout_under_test_is_not_the_default` catches it. That is the vacuity this repo keeps paying for,
>   reproduced deliberately.

**Verified, not quoted.** `egui_dock` 0.21.1's `Cargo.toml:48-53` has `serde = ["dep:serde", "egui/serde"]`;
`src/dock_state/mod.rs:44` derives `Serialize`/`Deserialize` on `DockState<Tab>` under it, over
`surfaces: Vec<Surface<Tab>>`, with `translations` `#[serde(skip)]`.

**When it turns on:** at the *end* of the parcel that removes the last placeholder tab and stabilises the
`Tab` enum — i.e. after P1-P3 ship, not with them. Not because a saved layout is expensive, but because the
`Tab` enum is what gets saved. *(This condition was met at `9c23365`, when the Objects panel made all five
of `Screen | Pacing | Registers | Memory | Objects` real.)*

**What migrating a layout saved too early would cost, concretely.** `DockState<Tab>` serializes the `Tab`
values themselves, so a saved layout is a tree with `"Registers"`, `"Screen"`, `"Pacing"` embedded in it as
serde's default external tagging of unit variants. A plain externally-tagged enum **errors on an unknown
variant** — serde offers no catch-all for that shape (`#[serde(other)]` is internally-tagged only). So:

* Rename or remove `Tab::Registers` and an old saved blob does not lose one tab — **the whole `DockState`
  fails to deserialize**, and the user loses their entire layout.
* The cheap, honest remedy is a version integer stored beside the blob, discarded wholesale on mismatch: the
  user gets the default layout back with no error, which is the right behaviour for a layout and the wrong
  behaviour for a document. It costs a few lines and no migration code.
* The expensive remedy is a hand-written `Deserialize` for `Tab` mapping unknown names onto a
  `Tab::Unknown(String)` placeholder, which then has to render *something*. Not worth it for a layout.

**And the enabling cost is not "one feature flag"** — see §9.2. It is two (`egui_dock/serde` **and**
`eframe/persistence`, which is *not* an eframe default: `eframe-0.36.1/Cargo.toml:60-69` lists
`accesskit, default_fonts, links, wayland, web_screen_reader, wgpu, winit/default, x11` and
`persistence = ["egui-winit/serde", "egui/persistence", "ron", "serde"]` is not among them), plus derives on
`Tab`, plus `App::save`/`CreationContext::storage` wiring. It pulls `serde` and `ron` into a crate whose
dependency list today is `oracle-core, eframe, egui, egui_dock, ringbuf, cpal` — zero serde.
(Persisting by hand with `serde_json` to a config path avoids `ron` and eframe's storage but not `serde`.)

---

## 7. What parcel 2 deliberately excludes

| excluded | why |
|---|---|
| ~~**P4-P6 (breakpoints / watchpoints / profiler)**~~ | §5.4. One shared run-loop change that re-opens parcel 1's pacing measurement. Parcel 3. **No longer excluded — the seam shipped as parcel 3 (§5.6) and the three tabs shipped on top of it (§5.7). The run-loop change was made once and re-measured once, as §5.5 asked.** |
| ~~**Layout persistence**~~ | §6. Turned on when the `Tab` enum stopped moving, which it did at `9c23365`. **No longer excluded — built; see §6's box.** |
| **`scanlines` as anything but an action** | §2.3. 438 KB/frame. |
| **A macro / input-timeline editor** (`press`, `play_input`, `hold`) | §2.2. Both are `require_paused`, so they are refused in the state a human would use them; a real timeline editor is a feature, not a panel. |
| **`object_spawn` and the spawn/move/delete surface** | Not on `main` (§1.2) — it is another lane's branch. When it merges its surface is a **click in the Screen tab**, per the ruling, sharing the click handler `object_at`/`pixel_attribution` already need. Designing that click for two consumers now and wiring the third later is free; guessing the method's shape now is not. |
| **The eleven catalogued-but-unserved methods** | §1.2. Audio spectrum, channel states, VGM, log tail, `z80_registers`, `ping`, `clicked`. None is served; a panel for an unserved method is exactly the item-19 violation inverted. |
| **Moving `Machine` to its own thread** | Parcel 1 named this as the escape hatch and made it cheap by keeping egui out of `machine.rs`. It should be taken when something is *measured* to stall, not before. Nothing in P1-P3 is a candidate: a hex view over a 64 KB window and a decoded object table are small. |
| **Any change to `oracle-frontend`** | The minifb player keeps its lenses. Porting the *model* halves of `lens/cpu.rs` and `lens/watch.rs` into shared code is an R1 opportunity for parcel 3, not a parcel-2 obligation. |
| **Reaching an emulator MCP tool from this design** | Standing prohibition; and §9.5 is about the MCP surface, not a use of it. |

---

## 8. UNMEASURED

Everything I could not establish in this worktree, gathered, not smoothed.

1. ~~**Nothing here was compiled or run.** Docs-only parcel; the cargo lane is held. Every `~lines`, every
   "≈10 lines", and the `Host::call` signature are *designs*, not builds.~~ **Overtaken by every parcel
   since**: 2b/2c built the panels, parcel 3 the seam (§5.6), parcel 3-tabs the three stopping tabs
   (§5.7) — all compiled, tested and measured. `Host::call` exists at `host.rs:533`. Struck here rather
   than left standing, because a stale UNMEASURED row reads as a live one. *(Items 4 and 6 were likewise
   answered by parcels 2b and 3 — see §5.4's box and §5.6.1 — and are left as written; they are those
   parcels' rows to strike, not this one's.)*
2. **The per-frame cost of every panel body proposed.** ~~Parcel 1 measured a `ui` bucket against a 20-row
   monospace grid. A 5-space hex view, a decoded object table and a sprite table are unmeasured. The
   design's claim that they are cheap is reasoning from size, not measurement.~~
   > ⚑ **PARTLY MEASURED as of parcel 3-tabs — §5.7.1 — and the item stays OPEN.** The three stopping
   > bodies were measured, and the reasoning-from-size claim is **falsified inside the Watchpoints /
   > Profiler pair** — which of the two, the measurement cannot say. What is now measured, and what each
   > number is:
   >
   > * **Breakpoints, with sixteen rows: ≲0.2 ms** (`+0.217 ms` of `ui-build` in the one clean sitting).
   >   Isolated — the shipped default arrangement draws that body and neither of its neighbours.
   > * ~~**Watchpoints + Profiler, with rows: +14.742 ms *together*, ~88 % of a frame budget.** A
   >   **residual of two**: no arrangement `--dock` can build draws one without the other. **Splitting it
   >   is still owed**, and so is the confirmation of §5.7.1's hypothesis about which body it is.~~
   >   > ⚑ **SPLIT AND FIXED — §5.7.2.** It was **the Watchpoints hit log, all of it**, established by
   >   > running two binaries each carrying exactly one of the two fixes: virtualising the hit log alone
   >   > recovers **+14.779 ms** and 60 fps, fixing the Profiler alone recovers **−0.057 ms** — nothing.
   >   > **Watchpoints, hit log at the 4096 ring cap, unvirtualised: +14.779 ms.** **Profiler, isolated
   >   > with the hit log already cheap: +0.027 ms** — 0.16 % of a frame, so §5's reasoning-from-size
   >   > claim is *confirmed* for that body. Both panels are now measured individually; this bullet is
   >   > closed.
   > * ~~**The whole eight-tab arrangement, armed: `ui-build` 15.220 ms median**, against 0.173 ms for the
   >   shipped default arrangement unarmed. In that state the player **does not hold real time** (51.6
   >   emulated fps, 1754 rebases, 1353 steady audio starvations). Measured, named, unfixed.~~
   >   **Fixed and re-measured (§5.7.2): `ui-build` 0.573 ms in that same configuration, 60.050 emulated
   >   fps, 0 rebases, 0 steady starvations.**
   >
   > **What is STILL unmeasured, and this item is not closed until it is:**
   >
   > * **Registers, Memory and Objects individually.** The only figure touching them is
   >   `ui-build` B − A = **+0.088 ms for four bodies at once** (Memory, Objects, and two *empty* stopping
   >   ones) at one-eighth of the window each — a **lower bound on two of them**, not a cost of either.
   >   §5.6.1's `ui-build` 0.140 ms *included* the Registers body — parcel 2's `initial_dock` drew
   >   Screen, Pacing and Registers, with Memory and Objects inactive behind Registers in their leaf —
   >   but as part of a three-body total that was never differenced, so it is not this item's answer
   >   either.
   > * **The Sprites panel** (§2.1) has not been built, so its body cannot be measured at all.
   > * **A human-sized pane.** Every figure here is from `every_tab_dock` or the default dock. A hex view
   >   or an object table given half the window lays out more rows than either arrangement gave it.
3. **The serialized byte size of a real `registers` / `object_list` / `object_slot` reply.** Only `scanlines`
   is derived (§2.3), and that derivation is arithmetic over the handler's `json!` shape, not a captured
   payload.
4. **Whether `Host::call` has a hazard I have not seen.** It does not exist. Its swap is `pump`'s swap and its
   dispatch is `pump`'s dispatch, but I have not looked for a re-entrancy or event-ordering interaction with
   `pending_free_run` / `pending_break`, both of which `pump` applies at the top of a drain and a bare `call`
   would not.
5. **Whether the ~440 KB `scanlines` figure was ever captured on the wire.** I derived it and it agrees with
   the ruling's number; I did not find a measurement of it anywhere in `docs/`.
6. **Whether the player can host a `Host` without moving the pacing numbers.** §5.4 asserts it moves them.
   That is a prediction from the fact that the inner loop gains wrappers and a drain, not a measurement.
7. **Whether `oracle-player` depending on `oracle-aether` unconditionally has a build-graph consequence.**
   `oracle-frontend` makes it optional for `--no-default-features`; `oracle-player` has no such gate that I
   found, but I did not build the graph.
8. **The MCP shim's live tool count.** §9.5 reads the shim's *source* in `oracle-old` (a sibling repo) and
   this session's own tool listing. I did not spawn a binary and call `initialize` — the standing bar says
   that is the only authority on what a consumer reaches, and I could not run it.

---

## 9. Corrections

### 9.1 The ruling's parity mechanism, as written, is not what this repo does

`docs/OVERSEER.md:690-694` says routing panels through the served surface makes the `pick.rs` /
`pixel_attribution` invariant true "by construction". **What makes it true there is one implementation under
two consumers, and the contract's D15 argues explicitly against the routing** — `pick.rs:645-655` is where
both halves are written down, and it decided *against* the round-trip on this exact question. Our own
`Host::pump` makes the case stronger than D15's general one, because the round-trip here is a queue and a
frame, not just a socket. **The ruling's requirement is right and this design implements it in full; its
proposed mechanism is the one the repo already rejected**, and §4.4 substitutes the one it adopted. I am
flagging this rather than quietly building the other thing.

### 9.2 `ui.rs` and the pacing doc understate layout persistence

`crates/oracle-player/src/ui.rs:150-155` — *"is one feature flag and a `Serialize` bound on `Tab`"*;
`docs/2026-09-02-player-pacing-design.md:203-205` — *"it is one feature flag and a bound on `Tab`"*.

It is **two** feature flags — `egui_dock/serde` and `eframe/persistence`, the latter verified *not* to be an
eframe default (§6) — plus derives, plus `App::save`/storage wiring, plus `serde` and `ron` entering a crate
that has neither. The conclusion both documents draw (don't do it yet) is right; the cost they quote is low
by roughly a dependency graph.

> **Confirmed by building it** (see §6's box). Two flags exactly, and the parenthetical below about
> hand-rolling with `serde_json` to avoid `ron` turns out **not to be available**: a `DockState` holds
> non-finite `Rect`s, which JSON cannot represent. `ui.rs`'s comment has been rewritten; the pacing doc's
> line 203-205 is left as the historical record of the same understatement.

### 9.3 Parcel 1's Registers placeholder already disagrees with `emulator/registers`

`ui.rs:126-146` renders `D0-D7`, then `A0-A6` with the comment *"A7 lives in usp/ssp on this core"*, then USP,
SSP, PC, SR. `Engine::registers` (engine.rs:2598-2613) emits `d0-d7`, **`a0` through `a7`** — where `a7` is
`r.addr_reg(7)`, which `registers.rs:64-70` resolves to the *active* A7 (SSP in supervisor mode, USP in user)
— plus `pc`, **`sp`**, `usp`, `ssp`, `sr`.

So the window shows nineteen values where the tool shows twenty-one, and **the two the window omits are the
two a human actually wants**: the active A7 and the SP. The comment is not wrong about the storage; it is wrong to
conclude the value cannot be shown, because `addr_reg(7)` is exactly the accessor that shows it. This is a
live instance of the defect the ruling's correctness half names — in the very panel it names — and P1 fixes it
in about four lines.

### 9.4 A client's held buttons are invisible to the human at the keyboard

> ### ✅ **BOTH HALVES BUILT, on `parcel/held-pads`** (`HELD-PADS-PLAYER`). The section below stands as the
> argument, and everything it describes as absent is now present. What shipped, and the two things the
> section did not anticipate:
>
> * **The field.** One read-only row in `StatusStrip`, sourced from `Host::held(0)`/`held(1)` through
>   `Bus::held_pads`, shown only when non-empty — and **first**, because every other row answers *what is
>   loaded and where is the machine*, questions a reader arrived with, while this one answers *is this
>   window still doing what my hands tell it*, which is the question they have not thought to ask and the
>   reason they are reading the strip at all. It renders
>   `port 0: left, start — a bus client is holding these, not you; emulator/release_all clears them`. The
>   remedy is **named in the row** precisely because this section says it "is one call, but you have to
>   know to make it": the row is where you learn it. A bare `left` would have been correct and would have
>   drawn *"what are the purple boxes"*.
> * **One vocabulary, not two.** `engine::held_names` — the function `emulator/hold`'s reply `held` array
>   is built from — is `pub` as of this parcel, and the row is built from it. A panel spelling the eight
>   button names for itself would have been a second vocabulary that agrees until somebody adds a ninth.
> * **⚑ What this section did not say: the sibling in §5.6.2 had to land in the SAME parcel.** Half 1
>   without half 2 makes the defect worse rather than better — while `hold` does nothing, nothing is
>   silently wrong; applied and unshown, it is this section's trap, built.
> * **⚑ What this section got right and one thing it did not.** The field *is* a field and not a tab, as
>   claimed. But the argument here rests on `Bus::merge_held` being applied — which in `oracle-player` it
>   was not, and the section did not know that; §5.6.2 did. The two were written as separate defects and
>   are one.
>
> `Engine::set_live_pads`' doc is at `engine.rs:1385`, not `:843` — §0's line-number caveat, again.

`Bus::merge_held` ORs a client's held set into the pad before the run (bus.rs:163-171), and
`Engine::set_live_pads`' doc (engine.rs:843-845) is explicit that `held` must stay *exactly what the client
asked for* and must never absorb the human's input. Correct — and it means a client that calls
`emulator/hold {buttons:["left"], down:true}` and disconnects leaves a human holding a controller that walks
left forever, with **nothing on the window that can say why**. `release_all` exists and is one call, but you
have to know to make it.

This is not in the owner's ruling and it is not a tab. It is one read-only field in the status strip, sourced
from `Host::held(0)`/`held(1)`, shown only when non-empty. I am raising it because the ruling's default —
*a served capability is reachable in the window* — has a mirror image the ruling does not state: **a served
capability that changes what the window does must be visible in the window.** `hold` is the only method on the
surface that silently alters what the human's own hands do.

### 9.5 Three served methods are reachable from no consumer but the raw socket

Derived, in this session:

* Source serves 56 (§1.1).
* This session's MCP tool listing exposes 53 — missing `emulator_breakpoint_set_enabled`,
  `emulator_object_at`, `emulator_screen_text`.
* The shim (`oracle-old/linux-port/mcp/oracle_mcp.py`) filters its table by the live `methods` list
  (`served_methods()`, line 1180; `list_tools()`, line 1220) — so a missing tool could be a stale binary. **It
  is not.** `grep` over that file finds `breakpoint_set_enabled` twice, both as `BREAKPOINT_HANDLE_MARKER` (a
  capability discriminator, line 1634), and finds **`object_at` and `screen_text` zero times**. The shim has
  no rows for them; no binary, however fresh, can surface them.

So `emulator/object_at` — the method the owner's ruling points at as the Screen panel's click target — is
today reachable **only** by a raw socket client. Parcel 2's Screen-tab click would be its first real consumer.
The defect is in `oracle-old`, not here; I am reporting it, not fixing it, and it strengthens the case for P3
and the click handler rather than weakening anything.

### 9.6 Two smaller ones, for the record

* **`docs/2026-09-02-toolkit-spike.md` §5.1's table is a curated 54 of the 56**, and says so ("the ones a
  panel app would want"). The two absent from it are `state_hash` and `wait_for_break`. Its headline count of
  **56 is correct** and reproduces exactly (§1.1). No correction needed — noted so a later reader summing the
  table does not conclude the count is wrong.
* **`docs/OVERSEER.md:129-133`'s "the banner says 52, and `initialize`'s `methods` array has length 52"** is a
  correctly SHA-anchored historical measurement at `6031020`, and is *not* an error. It reads as a current
  fact on a skim. The surface is 56 at `420c76d`. This is, pleasingly, the same entry that argues a published
  total is the wrong observable — which is why nothing in this design keys on one.
