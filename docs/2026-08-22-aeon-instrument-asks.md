# Four peer instrument asks, priced (2026-08-22)

**Base:** `b2be928` · **Branch:** `aeon-asks-survey` · **Method:** read-only survey. No cargo was run (a
peer agent held the cargo lane); no emulator MCP tool was touched. Every claim below is derived from
source at a named path and line, or from a sibling repo at a named revision. Where a claim could only be
settled by running a machine, it is TAGGED rather than guessed.

**Revisions verified against.** aeon `origin/master` = `33cbcf5` (2026-08-22). empyrean `origin/main` =
`40eb297` (2026-08-22). sigil and oracle-old revisions are cited in §5.

---

## 0. The headline

Three of the four asks are **already served by this repo's Rust Aether server**. What aeon and sigil need
is mostly a **migration date, not a work order**.

| | ask | triage | what it actually needs |
|---|---|---|---|
| **1** | deterministic framebuffer capture | **composable-today** + one genuinely-new sliver | cutover + a cross-process pixel determinism gate |
| **2** | per-frame union of live DMA enqueues | **composable-today** | cutover + a script; a better core surface is optional |
| **3** | cycle attribution across VBlank preemption | **satisfied-by-in-flight** | cutover only — the defect they describe is the legacy instrument's, and ours is tested against it by name |
| **4** | queryable per-instruction 68000 cycle figure | *(see §5)* | *(see §5)* |

### 0.1 The structural fact, verified firsthand

The brief's central structural claim is **correct**, and I verified it rather than assuming it. The
`mcp__oracle__*` MCP surface is configured in `~/.claude.json` as:

```
"oracle": {"type":"stdio","command":"/home/volence/sonic_hacks/oracle-old/linux-port/mcp/oracle-mcp"}
```

That is the **legacy C++ port**, not this repo. So every MCP-shaped complaint aeon reports about
screenshots, CRAM latching or profiler attribution is a complaint about **oracle-old**, and none of it
transfers to our server without being re-derived. This is why three of four asks triage to "cutover".

### 0.2 The served/unserved census, enumerated

Two independent derivations, both re-run for this survey:

- **Structural** — parsing `name:` out of the `METHODS` table (`crates/oracle-aether/src/engine.rs:204`),
  the single dispatch path (`Engine::dispatch`, `:984`, refuses anything absent with `-32601`): **40 served**.
- **Literal** — `grep -o '"emulator/[A-Za-z0-9_]*"' crates/oracle-aether/src/engine.rs | sort -u`: **43**,
  minus the three `EVENTS` (`engine.rs:453` — `emulator/stopped`, `emulator/resumed`,
  `emulator/romReloaded`): **40**. The `[A-Za-z0-9_]` class matters; `[a-z_]` hides `romReloaded`.

58 contract fragments − 40 served = **18 unserved**, and the pinned set
(`crates/oracle-aether/tests/schema_conformance.rs:393-417`) holds exactly those 18. Enumerated, because a
count whose elements are not listed drifts:

`emulator/audio_spectrum`, `emulator/breakpoint_add`, `emulator/breakpoint_clear`,
`emulator/breakpoint_list`, `emulator/get_channel_states`, `emulator/get_layer_states`,
`emulator/log_clear`, `emulator/ping`, `emulator/run_to_scanline`, `emulator/set_channel_enabled`,
`emulator/set_layer_enabled`, `emulator/vgm_start`, `emulator/vgm_status`, `emulator/vgm_stop`,
`emulator/wait_for_break`, `emulator/write_vram`, `emulator/z80_read`, `emulator/z80_write`. **(18)**

> **Correction to the brief.** The brief says 18 unserved and ~37 served. 18 is right; the served figure
> is **40**, not 37. The `step`/`step_over`/`step_out` trio left the pinned set on 2026-08-22 by being
> served (`schema_conformance.rs:405-409`), which moved served 37 → 40 and unserved 21 → 18. Both halves
> of the arithmetic must move together, and the brief moved only one.

The 40 served, enumerated: `checkpoint`, `checkpoint_drop`, `checkpoint_list`, `get_profiler`,
`get_profiler_frames`, `hold`, `load_symbols`, `lookup_symbol`, `memory_hash`, `pause`,
`pixel_attribution`, `play_input`, `press`, `read`, `read_cram`, `read_memory`, `read_vram`, `registers`,
`release_all`, `reload_rom`, `reset`, `restore`, `resume`, `run_frames`, `run_to`, `scanlines`,
`screenshot`, `set_profiler`, `sprites`, `state_hash`, `status`, `step`, `step_out`, `step_over`,
`watchpoint_add`, `watchpoint_clear`, `watchpoint_hits`, `watchpoint_list`, `write_cram`, `write_memory`
(all prefixed `emulator/`).

### 0.3 Headless is not an open question — aeon already does it

aeon's binding constraint is that their subagents cannot use MCP, so an instrument must be a headless bus
script. **That constraint is already satisfied, and aeon has already satisfied it seven times over.** At
aeon `33cbcf5`, these tools spawn our Rust `oracle-aether` as a subprocess over a unix socket, with no
MCP, no window and no GPU:

1. `tools/boot_override_gate.py:76`
2. `tools/hblank_window_sweep.py:139`
3. `tools/sh_probe.py:16`
4. `tools/staging_lifetime_timeline.py:567` (requires oracle-aether at/after CR-28)
5. `tools/tick_variance_probe.py:113`
6. `tools/vsplit_landing_gate.py:99`
7. `tools/warp_mailbox_gate.py:116`

The launch shape they use (`warp_mailbox_gate.py:151-167`):

```python
subprocess.Popen([SERVER, rom, "--socket", sock, "--no-pace"],
                 stdout=DEVNULL, stderr=DEVNULL)
```

`crates/oracle-aether/Cargo.toml` pulls `oracle-core` + `serde_json` only — no windowing, no audio, no
`oracle-frontend`. **Nothing about running headless is unsolved.** Answering "can it be headless?" with
"yes, and here are seven of your own scripts already doing it" is the whole of that question.

> **Correction to the brief.** The brief says "aeon's probes talk to the legacy server today". Half true
> and worth stating precisely: the **cost/profiler** probes do (they `sys.path.insert` the
> `oracle-old/linux-port/harness` module), but the **pixel and streaming** probes already talk to ours.
> The cutover is partial and in progress, not pending.

---

## 1. ASK 1 — deterministic framebuffer capture

### Triage: **composable-today**, plus one genuinely-new sliver worth building

Everything aeon asked for is reachable from methods this server already serves. The one thing that does
**not** exist and should is a *cross-process, pixel-level* determinism gate — see §1.6.

### 1.1 Is the core deterministic by construction? Yes, and the crate says so out loud

`crates/oracle-core/src/lib.rs:8` states the invariant as a crate-level rule:

> No `HashMap` or floats in hashed/serialized state; zero threads in core.

Enumerating what I searched for, how, and what came back — because "I found none" only means something
when the search is named:

| nondeterminism source | search | result in `oracle-core/src` |
|---|---|---|
| host / wall-clock time | `grep -rn "Instant::now\|SystemTime\|elapsed()\|thread::sleep" crates/oracle-core/src/` | **exit 1 — zero matches.** No wall-clock anywhere in the core. |
| threading | crate doc (`lib.rs:8`) + the above | "zero threads in core"; no `thread::spawn` reachable from the sweep |
| hash iteration order | `grep -rn "HashMap\|HashSet" crates/oracle-core/src/` | **4 matches, all prose forbidding it**: `lib.rs:8`, `scheduler.rs:5` ("`BTreeMap`, never a `HashMap`, so iteration/pop order is deterministic"), `symbols.rs:440-441` ("the crate bans `HashMap` in anything that might be hashed or serialized"). No `HashMap` in state. |
| randomness | `crates/oracle-core/src/rng.rs` read in full | **SplitMix64, hand-rolled, dependency-free**, "the *only* source of randomness in the core (it seeds power-on RAM/VRAM). Deterministic by construction." No `rand` crate. |
| uninitialised memory | `System::new` (`system.rs:409-419`) | RAM and VRAM are **filled from the seeded RNG in a pinned order** — work RAM first, then `Vdp::power_on` draws from the same stream, "the exact pre-extraction order, so the power-on `state_hash` is byte-identical". Not `MaybeUninit`; not zeroed-and-hoped. |
| **the seed itself** | `grep -rn "System::new(" crates/oracle-aether/src/ crates/oracle-frontend/src/main.rs` | **`0x5EED`, a hardcoded constant, in all three arrangements**: server `main.rs:59`, player `main.rs:835`, and the doc example `lib.rs:55`. `host.rs:200`'s `System::new(0)` is the inert placeholder that "never runs a single instruction" and is exchanged out on every `pump`. |
| wall-clock pacing | `grep -rn "free_run_pace"` | **exactly one consumer**: `server.rs:467-476`, the free-run loop, `Instant::now()` + `thread::sleep(rest)`, gated on `Config::free_run_pace` (`engine.rs:136`, default `Some(16_667µs)`, `:176`). `--no-pace` sets it to `None` (`main.rs:70-71`). |

**The one real determinism rule that follows, and it must be written into any gate:** pacing does not
change what a frame computes, but **free-running does change how many frames run per unit wall-clock**.
So a deterministic capture must never free-run — it must `pause` and then `run_frames`/`play_input`.
The server already enforces the safe direction: `require_paused` (`engine.rs:1613`) refuses
`run_frames`/`run_to`/`step*`/`play_input` while free-running with `-32005 machineRunning`, so a script
cannot drift into the nondeterministic mode by accident.

**Existing evidence in tree.** `crates/oracle-core/tests/determinism_gate.rs` — "the most-guarded CI job"
— runs two fresh in-process instances for 120 frames and asserts the per-frame `export_state_hash`
sequences are byte-identical. `crates/oracle-core/src/system.rs:1911` pins
`new_is_deterministic_for_same_seed` (two `System::new(0xC0FFEE)` compare equal, including `state_hash`).

**What that particular gate does NOT catch** (`determinism_gate.rs:8-15`, its own words): it is
in-process, and it hashes `export_state` — CPU regs + work RAM — **not pixels**. Its own doc books the
missing leg: *"Oracle's version spawns two separate processes over the bus to also catch process-level
nondeterminism; that over-the-bus port lands with `oracle-bus`."* That port has not landed.

**But the pixel half is already gated, and much harder than I expected before reading it.**
`crates/oracle-aether/tests/scanlines.rs:256-281`, `a1_determinism_three_boots_byte_identical`:

- **three separate `Server` instances, three separate unix sockets, three separate engine threads**, each
  with its own `System::new(0x5EED)` + ROM + `reset` (`tests/common/mod.rs:33-55`);
- six frames each, then `emulator/scanlines {}` — **the whole 224-line frame**;
- asserts `source == "raster"` per boot (the liveness guard), all three replies **byte-identical** as
  serialized JSON, and that the comparison covered `ACTIVE_LINES` rows rather than a stripe.

Add `crates/oracle-core/tests/scanline_goldens.rs`, which boots all 17 vendored TestRoms, runs 120
frames with a `Retain::LastFrame` capture and hashes **the frame as the VDP actually drew it**, pinning
six ROMs as `LIVE-DIFFERS` with explicit hashes and eleven as identical to a post-hoc render — with four
anti-vacuity guards of its own, including one that XORs a single bit at line 111 and requires the hash to
move (`:360-382`).

So the honest position is: **pixel determinism at the wire is already gated across three independent
servers and pinned against goldens in core.** The only legs genuinely missing are separate OS *processes*
and a wire-level pinned golden — see §1.6, which is correspondingly smaller than I first drafted.

### 1.2 The pixel surfaces — all three are SERVED on the Rust server

Every pixel surface aeon named is on our server today. None of them is in the unserved 18.

**`Engine::framebuffer()` (`engine.rs:1588-1600`) is the single source all of them read.** It returns the
retained last completed raster frame when there is one (`from_raster = true`), and otherwise re-renders
from current VDP state (`from_raster = false`). Every advancing path goes through the screen capture and
then `latch_screen()` (`engine.rs:978`, and the doc comment at `:985-989`: "that is what makes
`emulator/screenshot` scanline-accurate rather than a post-hoc guess").

| surface | served | what it returns | frame-exactness |
|---|---|---|---|
| `emulator/scanlines` (`engine.rs:3001`) | **yes** | **the whole active display by default.** `startLine` defaults 0, `count` defaults `224 − startLine`, so `{}` returns all 224 rows as `{line, width, rgb}` with `rgb` a hex byte string. `mode` is derived from the answering frame's own width, never from a register. Bounds are **refused, never clipped**. | per-line raster when `source == "raster"` |
| `emulator/state_hash {includeFramebuffer:true}` (`engine.rs:2396`) | **yes** | an **FNV-1a of the full RGB framebuffer** plus `framebufferSource` (`:2429`) | same |
| `emulator/screenshot` (`engine.rs:3061`) | **yes** | writes a **PNG** and returns `{path, format, width, height, bytes, source}` | same |
| `emulator/pixel_attribution` | **yes** | per-pixel plane/sprite attribution | frame-state-based |

**Three things worth flagging to aeon immediately, because each is a fact they are demonstrably operating
without:**

1. **`emulator/scanlines` with `{}` gives them the whole frame.** `warp_mailbox_gate.py:135` uses
   `SCANLINE_START, SCANLINE_COUNT = 100, 8`. They are taking 8 rows because they believe that is what
   the method is for. It is a full-framebuffer readback.
2. **`emulator/scanlines` is NOT bounded by `limits.maxReadLen`.** Their comment at
   `warp_mailbox_gate.py:96` correctly notes `read_vram` caps at 4096; `scanlines` has no such cap, only
   the 224-line ceiling. A full H40 frame is 224 × 320 × 3 = 215,040 bytes, ~430 KB as hex on the wire in
   one reply. That is large but is one call, and `state_hash{includeFramebuffer}` is the cheap form when
   only a comparison is wanted.
3. **`emulator/screenshot` also carries `source`.** `warp_mailbox_gate.py:84` says `emulator/scanlines`
   is "the only pixel readback with a `source` field". At `b2be928` that is no longer true —
   `screenshot` emits `"source"` at `engine.rs:3087` and `state_hash` emits `"framebufferSource"` at
   `:2429`, both from the same `from_raster` flag.

**Is PNG encoding deterministic?** The encoder is **ours** — `crate::png`, in-tree, adopted specifically
so "this crate's runtime deps stay `oracle-core` + `serde_json`" (`engine.rs:3070-3072`). No libpng, no
timestamp chunk from a third party. That said, for a gate I recommend **not** capturing through PNG at
all: `state_hash{includeFramebuffer:true}` is one round-trip, one hex string, and hashes the raw RGB
before any encoder touches it. Use PNG for eyeballs, the framebuffer hash for the gate, and `scanlines`
for the diff when the hash disagrees.

**The one cross-surface hazard aeon has NOT been warned about, and it is sharper than either trap they
named.** `emulator/pixel_attribution` does **not** read the same thing as `screenshot`/`scanlines`. It
calls `sys.vdp().pixel_attribution(x,y)` (`crates/oracle-core/src/render.rs:1627-1682`) — live VDP state,
post-hoc — while `screenshot`/`scanlines` read the retained raster frame. On any raster-effect ROM they
legitimately disagree, and `engine.rs:2278-2282` says so outright: *"this and `emulator/screenshot` can
legitimately disagree — and pausing does **not** reconcile them."* **Do not cross-validate one against
the other.** A gate that did would be a third failed capture protocol.

Two corollaries in the same family:

- `emulator/write_cram` is a `poke_cram` (`vdp.rs:1567`) that explicitly **does not repaint a frame
  already drawn** (`engine.rs:2146-2147`). It moves `pixel_attribution` immediately and moves
  `scanlines`/`screenshot` only after another `run_frames`.
- The magnitude of the live-vs-post-hoc gap is not academic. `scanline_goldens.rs:120-141` measures
  `color_1536` performing **515 value-changing CRAM writes inside the active-display window** across 131
  lines in one frame: the live picture holds ~1400 distinct colours, the post-hoc one holds **4**. And
  `window_distortion` turns **one** mid-frame R17 write into **112** wrong lines under a post-hoc render.

### 1.3 The two traps aeon warns of — both already answered here

**Trap 1: frame-latched CRAM.** This is a **legacy defect that our server does not have**, and this repo
established that on 2026-08-17 (`docs/2026-08-17-aeon-switchover-gap-list.md`, "Verified" §):

> **Their Tier-2 item 7 question is answered YES.** `emulator/read` with `space:"cram"` reads
> `sys.vdp().cram()` — live committed state, no frame latch. **Oracle's frame-latched CRAM read was
> the vacuous instrument they suspected; ours is not.**

The deeper structural answer, which matters more than the read path: **our pixels do not come from a CRAM
read at all.** When `source == "raster"` each retained row is decoded against **the 128-byte CRAM
snapshot taken at that row's line start**, then walked segment by segment through that line's own journal
of CRAM writes — each write mapped to a landing pixel by `vdp::subline_x` (`render.rs:487-514`,
`:688-743`). A row whose palette changed mid-scan therefore shows *both* colours, split at the right
column. So a capture built by reading CRAM and re-colouring would be the vacuous instrument; a capture
built on `scanlines`/`state_hash` cannot be, because it never consults live CRAM.

The renderer guards this by name: `Vdp::report_rgb`, which *does* decode against live CRAM, carries an
explicit prohibition at `render.rs:1716-1720` — *"**Do not call this on a retained report.** … that
substitution is exactly the mutation that moves the `scanline_goldens` scorecard."*

**The discipline that makes it airtight — and aeon already invented it.** When `source == "stateRender"`
the trap does bite: the frame is re-rendered from VDP state as it stands now, and the reply says so both
in `source` and in an explicit `caveat` (`engine.rs:3089-3097`, `:3078-3086`). So the rule for any gate
is **hard-fail on `source != "raster"`**, which is exactly what `warp_mailbox_gate.py:84-88` already
does. That should be lifted into the house recipe rather than re-derived per script.

`source` is `stateRender` in precisely one situation: no completed frame is retained — the machine has
not drawn one yet, or `reset`/`reload_rom`/`restore` dropped it. So: **after every `reset` or `restore`,
run at least one frame before capturing.**

**Trap 2: the uniform witness buffer.** Real risk, and cheaply defeated. Two mechanisms exist today:

- `emulator/scanlines` returns RGB per pixel, so a **distinct-colour count** over the returned rows is a
  one-line non-vacuity floor. aeon already does this (`warp_mailbox_gate.py:88-90`, "a distinct-colour
  floor taken from the REFERENCE run's own capture") — derived in-run, not typed, which is the right
  shape.
- The house precedent for pinning it is `crates/oracle-core/tests/golden_frames.rs`, which FNV-hashes
  full active-height framebuffers as pinned constants **and carries its own poison test**:
  `golden_frame_hash_discriminates` (`:332-339`) changes one plane-A cell and asserts the hash moves,
  "so the harness actually depends on the pixels". That is the pattern to copy.

### 1.4 Poison-testability — not a question. The poison is already built, shipped and pinned

An instrument that cannot demonstrate a red is not adoptable, and aeon is right to demand it. Ours
demonstrates one *as a shipped suite gate*, not as a thing we could add.

**`oracle_core::testrom::build_cram_midframe(line: u8)`** (`crates/oracle-core/src/testrom.rs:485`) is a
purpose-built poison fixture: it polls the HV counter and repaints the backdrop CRAM entry at a
caller-chosen scanline. `crates/oracle-aether/tests/scanlines.rs:304-402`
(`a2_two_timings_differ_and_the_boundary_moves`) builds two of them — line 50 and line 150, identical in
every other byte — and asserts four independently-failing properties:

1. `source == "raster"`;
2. within one frame, a row above the boundary differs from a row below it (a post-hoc render draws the
   whole frame in the last colour written, so this alone fails a blind server);
3. the landing row is **split** — uniform prefix, uniform suffix, **exactly one** transition, at a column
   inside a band **derived from the fixture's own 68000 instruction cycle costs** (`landing_band`,
   `:474-482`: the band is `46..=79`, the measured column 53, and the doc says *"do NOT widen the band to
   fit"*);
4. the two ROMs' bands are **swapped**.

That is the whole shape a peer should demand of us, and it is already the file's reason for existing.
A second gate, `restore_drops_the_frame_and_the_same_machine_answers_staterender`, reads one machine
point twice — once with a retained frame, once after `restore` dropped it — and requires disagreement.

**Reds available to aeon over the bus today, all with served methods:**

| poison | method | expected red |
|---|---|---|
| flip one palette entry, **then run a frame** | `emulator/write_cram` + `emulator/run_frames` | framebuffer hash moves. The `run_frames` is mandatory — `write_cram` is a `poke_cram` that does not repaint an already-drawn frame (`engine.rs:2146-2147`) |
| poke the camera / a VDP register | `emulator/write_memory` | scroll changes; whole frame moves |
| drop the retained frame | `emulator/reset` / `reload_rom` / `restore` | `source` flips to `stateRender` — proves the liveness check itself works |
| prove the input pin is real | drop a row from `emulator/play_input`'s `rows[]` | trajectory diverges |
| mid-frame repaint at a chosen line | the `build_cram_midframe` fixture (in-core, or shipped as a ROM) | the sub-line poison — the sharpest form |

**Two poisons that are NOT available, and both are worth stating rather than letting aeon discover:**

- `emulator/write_vram` is **unserved** (it is in the 18), so there is no direct VRAM poison on the bus.
- **Layer toggling does not exist anywhere in the Rust tree.** `rg -i 'layer_enabled|set_layer|layer_states'`
  over `oracle-core/src`, `oracle-aether/src` and `oracle-frontend/src` returns **exit 1, no match**.
  `emulator/set_layer_enabled` and `emulator/get_layer_states` are not merely unserved — they are
  unimplemented in core. That is a legacy-only capability. Use a CRAM poke or a scroll-register write.

**Non-vacuity primitive, ready to lift.** `scanlines.rs:404-432`'s `assert_split_row` parses a row's hex,
counts A→B transitions and asserts exactly one — *"zero means the row is uniform, i.e. line-atomic."*
That is the direct answer to aeon's "the witness buffer turned out uniform" trap, already written.

### 1.5 The composition, method by method

All served. This is ASK 1, complete, with no new wire surface:

```
1. emulator/reset                                       # cold start from the fixed 0x5EED power-on
2. emulator/write_memory  {...}                         # pin camera / scene state
3. emulator/play_input    {rows:[...]}                  # THE pin — see below
4. emulator/state_hash    {includeFramebuffer:true}     # the witness: one hash + framebufferSource
5.   assert framebufferSource == "raster"               # the anti-vacuity gate
6. emulator/scanlines     {}                            # full 224-line RGB, only when (4) disagrees
7.   assert distinct-colour count >= a floor derived from the reference run
```

Step 3 is the piece that makes "same inputs in, same bytes out" literal rather than aspirational.
`emulator/play_input`'s contract (`engine.rs:3152-3160`) is:

> **The pad at frame N is a pure function of `rows`, and of nothing else.** Both non-row sources are
> suspended for the duration and restored afterwards: the client's `held` set *and* the host's `live`
> input. […] "apply the rows on top of what is already held" is the easier implementation the contract
> had to forbid by name.

Combined with the constant `0x5EED` seed and the zero-wall-clock core, that gives determinism **across
processes and across machines**, not merely across two runs in one process.

### 1.6 Better-approach pass — what we should build beyond the floor

aeon's floor is "a capture that reproduces". Three things exceed it, in order of value:

1. **★ Extend the three-boot pixel gate to separate OS processes and to a pinned wire golden.** This is
   the only genuinely-new sliver, and after reading `a1_determinism_three_boots_byte_identical` it is
   much smaller than I first drafted. Two legs are missing, and only two:
   - **Separate OS processes.** `a1` runs three servers on three sockets and three engine threads, but in
     one process. `determinism_gate.rs:8-9` books the process-level leg explicitly and it has never
     landed. Separate processes are what catch ASLR-, allocator- and environment-dependent effects that
     a same-process comparison structurally cannot. Given `oracle-core` has zero `unsafe`, zero threads
     and zero host reads, I expect this to pass on the first run — which is exactly why it is cheap and
     worth having as a citable artifact rather than an argument.
   - **A pinned wire golden.** `a1` compares three boots *to each other*, so a change that moves all
     three identically passes. Core pins live-frame hashes (`scanline_goldens.rs`), the wire does not.
     One pinned `state_hash{includeFramebuffer:true}` over a `play_input` timeline closes it.

   **Cost: small — one test, no core change, no contract change.** It needs the release binary, so it is
   a harness question, not a bus question.
2. **A documented bound on what may legitimately vary.** aeon explicitly says a documented bound is a
   large win even if nondeterminism is intrinsic. Our honest bound is unusually tight and should be
   written down as the answer: *given the same ROM bytes, the same `play_input` rows, and a capture taken
   at a `run_frames` boundary with `framebufferSource == "raster"`, **no pixel may vary**.* The three
   escape hatches are all client-visible, not intrinsic: free-running (refused by `require_paused` for
   the methods that matter), a capture before the first completed frame (`source` says so), and a
   different power-on seed (constant `0x5EED` in every arrangement — a divergence here would be a bug,
   not a tolerance).
3. **Consider a raw-bytes framebuffer readback.** `scanlines` at ~430 KB of hex for a full frame is
   workable but wasteful for A/B loops. This would need an empyrean fragment and is **not** worth opening
   one for on its own — but if a fragment is being opened anyway, a `format: "raw"`/base64 option on
   `scanlines` is the natural rider. **Do not invent it unilaterally.**

### 1.7 Core readiness and cost

**Core readiness: ready.** Render path (`crates/oracle-core/src/render.rs`, `vdp.rs`), retained-frame
capture (`scanline_capture.rs`), seeded power-on, and pinned pixel goldens all exist and are gated.

**Cost:** the composition is **zero** — a script aeon writes. The bus-level determinism gate is **one
small parcel**, no contract CR. The raw-bytes option is **contract-first** and should wait for a reason.

### 1.8 What must go back to aeon on ASK 1

- Do their "three capture protocols that failed their own controls" refer to oracle-old only? If any was
  attempted against our server, we want the failing call verbatim — that would be a real defect and
  nothing above would explain it.
- Confirm they know `emulator/scanlines {}` returns the whole frame, and that `screenshot` and
  `state_hash` now carry `source`/`framebufferSource` too.
- Warn them explicitly not to cross-validate `pixel_attribution` against `scanlines`/`screenshot`
  (§1.2's cross-surface hazard) — that pairing is a fourth failed capture protocol waiting to happen.
- One operational caveat to pass on: a `SIGINT`/`SIGTERM`-killed server leaves its socket file behind, so
  a stale socket yields `ECONNREFUSED` rather than `ENOENT` (`main.rs:99-114`; pinned both directions in
  `tests/socket_lifecycle.rs`). It is never fatal — `Server::bind` probes and unlinks — but a script that
  reads "connection refused" as "server is broken" will be misled.

---

## 2. ASK 2 — per-frame UNION of live DMA-queue enqueues

### Triage: **composable-today** — no new bus method, no contract CR, no core change

The measurement that replaces the 2160 B ceiling is buildable now from served methods. A better core
surface exists and is worth building later, but it is an upgrade, not a prerequisite.

### 2.1 Their consumers, verified at aeon `33cbcf5`

- `engine/level/scene_dsl.emp:1941` — `pub const SB_AXIS2_RESERVATION = 2160 // CEILING: whole-region 3056 B less the scene's 896`
- `tools/effects_budget_model.toml:1117` — `axis2_reservation_bytes = 2160`; `:1272` maps
  `"scene_budget.axis2_reservation_bytes"` → `engine/level/scene_dsl.emp:SB_AXIS2_RESERVATION`
- `docs/DEFERRED_WORK.md:253-270` — the booking, with the unlock verbatim.

The structure that makes this tractable, also at `33cbcf5`:

- `engine/system/dma_queue.emp:52-54` — three queues, three cursor words (`DMA_Critical_Slot`,
  `DMA_Important_Slot`, `DMA_Deferrable_Slot`), each initialised to its queue base.
- `:287` — `ensure(sizeof(DMAEntry) == 14, …)`. Stride is pinned at 14 bytes.
- `:362` — the drain resets **only the cursor**, never the entry bytes. **That is the residue mechanism**
  aeon describes, and it is why reading queue *contents* at an arbitrary instant lies.
- `engine/system/vblank.emp:200, 205, 217` — drain order Critical → Important → Deferrable, all inside
  VBlank.

### 2.2 Pass A — the faithful instrument: a queue-write sweep

Measures *writes into the queue*, so residue is structurally impossible: a stale entry is not a write.

```
1. emulator/load_symbols   {"path": "<aeon>/s4.debug.lst"}
2. emulator/lookup_symbol  {"name": "DMA_Critical"} / "DMA_Important" / "DMA_Deferrable"
                           and the three *_Slot cursors
3. drive to the streaming-active state (emulator/play_input | press | run_frames),
   then emulator/checkpoint so every later pass replays the same state
4. emulator/watchpoint_add {"symbol":"DMA_Critical","len":<slots_c*14>,
                            "write":true,"mode":"record","label":"q-crit"}
   ... likewise the other two queues and the three 2-byte cursors
5. per frame:
     emulator/run_frames      {"frames": 1}
     emulator/watchpoint_hits {"limit": 4096, "cursor": <last seq>}
     group hits by hit.frame; reconstruct each DMAEntry from addr/value/size at base + k*14;
     union = sum over distinct (queue, slot k) touched that frame;
     ASSERT `dropped` unchanged;  track max over frames of (union − 896)
```

### 2.3 Pass B — the cheap cross-check: stop at the drain

`emulator/run_to` takes a **symbol** and stops *before* executing the target instruction
(`engine.rs:1709`; the predicate is fed the PC about to execute, `crates/oracle-core/src/bus.rs:312`).
Because `Process_DMA_Critical` is the first drain in the VBlank handler (`vblank.emp:200`), all three
queues are at their fullest at that instant, and `cursor − base` over 14 is the exact live entry count —
no residue, because you read only up to the cursor.

```
emulator/run_to      {"symbol":"Process_DMA_Critical","maxFrames":2}
emulator/read_memory {"symbol":"DMA_Critical_Slot","len":2}        -> cursor
emulator/read_memory {"symbol":"DMA_Critical","len":<slots_c*14>}
```

**Pass B's honest limitation, which is why Pass A is primary:** it gives queue *contents at one instant*,
not the *union of enqueues over the frame*. Two ways they differ, both nameable from aeon's own source:
(i) anything enqueued between the Critical drain and the Important/Deferrable drains is not in the
snapshot; (ii) `Deferrable` compacts survivors forward across frames (`dma_queue.emp:410-445`), so its
contents include entries enqueued in *earlier* frames, which inflates a per-frame reading. Pass A counts
writes and is immune to both. **Run both — they bracket the answer**, and a bracket is a better artifact
than either number alone.

**Pass B′ — the breakpoint substitute.** `emulator/watchpoint_add {"symbol":"DMA_Important_Slot","len":2,
"write":true,"stopAfter":N}` + `emulator/run_frames` halts at the next instruction boundary with
`emulator/stopped {reason:"watchpoint", watch}` (`engine.rs:1385-1406`), and `run_frames` honestly
reports `frames: 0` if it fired inside the first frame (`:1422-1427`).

### 2.4 The hit cap — checked specifically, and it does not truncate silently

Ring cap is `config.watch_ring_cap`, default **4096** (`engine.rs:175`), dropping oldest-first past it.
The under-report risk the brief worried about is real but **client-discipline, not an instrument defect**,
on three independent axes:

1. every drop increments `dropped`, surfaced on every `watchpoint_hits` reply
   (`crates/oracle-core/src/watchpoints.rs:669-683`, `engine.rs:4123`);
2. `seq` is monotonic and assigned before storage, so **a gap in `seq` is a visible drop marker**
   (`watchpoints.rs:689-691`, pinned by `ring_buffer_drops_oldest_and_counts_and_keeps_seq` at `:1077`);
3. `ringCap` is advertised in `initialize` (`engine.rs:1302`) *before* a client plans around it.

So the script must assert `dropped` is unchanged per frame. Volume sanity: a 14-byte entry written via
`movep` byte-lanes is order 10–20 bus writes, so even 100 enqueues/frame is ~2000 hits — under the cap.

**`emulator/watchpoint_hits` result fields, enumerated.** Top level (8, plus `cursor` when `truncated`):
`hits[]`, `total`, `returned`, `limit`, `truncated`, `dropped`, `seen`, `matched`. It is
**non-draining** — `hits()`, never `take_hits()` (`engine.rs:4056-4059`) — so it accumulates and pages
forward by `cursor`. Per hit (11, plus 3 conditional): `watch`, `space`, `addr`, `value`, `size`, `op`,
`via` (`bus`|`direct`|`dma`), `pc`, `frame`, `mclk`, `seq`; plus `fc` iff `space=="bus"`, `old` iff
`space!="bus"`, and `symbol`/`symbolDisp` when the PC resolves. **`value` and `pc` are both on the
wire**, which is what makes reconstructing a `DMAEntry` from raw writes possible.

**Two caps that do bite, and rule out the census route.** `censusKey` has only three wire spellings —
`"addr"`, `"value"`, `"via"` (`engine.rs:4415-4426`); core's `AddrPage(n)`/`Fc`/`Op`/`Size`/
`ValueHiEqLo` (`watchpoints.rs:200-227`) have none. And `keyCap` is **not settable over the wire**, so
every census runs at `DEFAULT_CENSUS_KEY_CAP = 256` (`watchpoints.rs:100`). A byte-exact union via
`censusKey:"addr"` is therefore capped at 256 distinct bytes per watch — loud when it caps
(`keysCapped:true`, `censusOverflow` counted) but useless as a union. **Use record mode, not census.**

### 2.5 Served vs unserved, for this ask

| method | served | evidence |
|---|---|---|
| `watchpoint_add`/`_clear`/`_list`/`_hits` | **yes** (4) | `engine.rs:305, 311, 317, 323` |
| `run_frames`, `run_to` (addr **or** symbol) | **yes** | `engine.rs:257, 263`; handler `:1697` |
| `read_memory`, `load_symbols`, `lookup_symbol` | **yes** | `engine.rs:335, 413, 407` |
| `checkpoint`/`restore`/`_list`/`_drop` | **yes** | `engine.rs:281-299` |
| `breakpoint_add`/`_list`/`_clear`, `wait_for_break` | **NO** | in the pinned 18; `capabilities.breakpoints: false` (`engine.rs:1280`) |
| `run_to_scanline` | **NO** | in the pinned 18 |

> **Correction to the brief.** The brief suggests "breakpoints (`emulator/breakpoint_add` +
> `emulator/wait_for_break`) to stop AT the drain routine". **That path does not exist on this server** —
> both are unserved and `capabilities.breakpoints` is hard-`false`. The substitutes are `run_to {symbol}`
> and watchpoint `stopAfter`, and `run_to {symbol}` is in fact *better* here: symbol-addressed, bounded
> by `maxFrames`, no arm/disarm state. Neither the brief nor aeon's own booking noticed that `run_to`
> accepts a symbol, which is what turns their option (a) into a two-call loop rather than a new hook.

### 2.6 Existing DMA modelling — inventory

**Hardware VDP DMA + FIFO: modelled in core, not exposed on the bus.** `crates/oracle-core/src/vdp.rs`:
the physical 4-slot write FIFO ring (`:207-233`, `fifo_enqueue` `:589`, `fifo_drain` `:706`), status bits
8/9 from `fifo_len` (`:480-485`), DMA-busy from `dma_busy_until` (`:486`), `DmaRequest` Mem/Fill/Copy
(`:1603-1611`), `arm_dma` (`:914`), and the per-line access-slot cost model (`:599-675`). `DmaRecord`
(`:1615-1632`) exists but `Vdp::last_dma` holds only the **most recent** transfer (`:242-244`), and
`FrameReport { dma: Option<DmaRecord> }` (`render.rs:575-578`) is likewise "the last one", not an
accumulation. Recon: `docs/2026-07-22-vdp-dma-cd5-recon.md`, `docs/2026-08-03-a3-dma-fifo-design.md` —
both conformance work; neither introduces byte accounting.

**The game's software DMA queue is modelled by nothing special** — it is ordinary work RAM, reachable by
`read_memory` and by a `space:"bus"` watch. That is the correct answer, not a gap: it is game data, and
conflating it with the hardware FIFO would be a modelling error.

**Per-frame accounting that exists:** the profiler's `FrameRow {frame, cycles, stall_cycles, hint_cycles,
vint_cycles}` (`profiler.rs:350-362`) — cycles only, no byte volume. `stallCycles`' schema description
enumerates the DMA bus-hold window as one of its three contributors, so the profiler knows *when* DMA
happened in cycle terms and never in bytes. `ScanlineCapture` deliberately arms no VDP write capture
(`scanline_capture.rs:314-315` asserts `!s.wants_vdp_writes()`).

**Conclusion: there is no per-frame DMA byte counter, no FIFO-occupancy surface, and no VDP
write-volume bucket anywhere on the bus.** `frame_report` has zero references in
`crates/oracle-aether/`.

### 2.7 Better-approach pass — what we should build

**Build a first-class per-frame VDP/DMA write-volume surface in core. Do not build a queue-shaped
method.** Ship it *after* handing aeon the composition, because the composition unblocks them today.

Why the core surface wins:

1. **It measures the scarce thing directly.** Axis 2's pool is H40 NTSC VBlank DMA capacity — a hardware
   transfer budget. Bytes actually pushed to VRAM per frame *is* that quantity. The queue is a game-side
   proxy for it.
2. **It is immune to all three hazards** that make the queue reading delicate: residue, drain
   scheduling, and `Deferrable`'s cross-frame compaction. Two of those have already cost aeon a wrong
   number (`scene_dsl.emp:1913-1926`).
3. **It is game-agnostic.** A queue-shaped method would encode `sizeof(DMAEntry) == 14`, three cursor
   names, and a drain-reset convention — all aeon's to change, one of which aeon already guards with an
   `ensure` precisely because it is fragile. We would ship a method a peer's refactor silently
   invalidates.
4. **The choke points already exist.** `Vdp::in_dma` (`vdp.rs:273`) is raised around
   `run_fill`/`run_copy`/`dma_write_word`, and every VRAM byte routes through the single
   `write_vram_byte` choke (`vdp.rs:784`).

Shape: extend `FrameReport` from `Option<DmaRecord>` to a per-frame rollup — bytes by target
(vram/cram/vsram), by mode (mem/fill/copy), direct-port bytes, and a **VBlank-vs-active split** on the
line boundary the renderer already knows. aeon's axis-2 number then becomes literally
`max over frames of vblank dma_bytes`, with no reconstruction step.

**Cost.** Core accounting: ~150–250 lines with tests; it is *observer* state and must stay out of both
frozen currencies exactly as `Watchpoints` does (`watchpoints.rs:10-13` states the rule). Bus surface:
one method + **a schema fragment negotiated with empyrean first** —
`schema_conformance.rs:423-432` makes an unvendored method a deliberate decision, not a drive-by.
**Total: 1–2 focused parcels, and one contract CR.** The payoff beyond the number itself is that the new
surface and the Pass-A composition cross-validate each other, which is strictly better than either alone.

**What I would not build:** a `DMA_queue`-reading method, and a per-hit push event for watchpoints
(`engine.rs:3859-3864` already rules the latter out on volume grounds — 4.9M CRAM writes over 120 frames
in one test ROM).

### 2.8 Core readiness and cost

**Core readiness: ready** for the composition (watchpoints, symbols, `run_to`, `read_memory` all live and
tested). **Absent** for the better surface (no byte accounting anywhere).

**Cost:** composition **zero** (aeon writes the script; no CR). Better surface **1–2 parcels + one
contract CR**.

### 2.9 What must go back to aeon on ASK 2

- The per-queue slot capacities (`slots_c`/`slots_i`/`slots_d`) — the script needs them for each watch's
  `len`. They are near `Init_DMA_Queue` in `dma_queue.emp`; one grep settles it.
- Confirm `DMA_Critical_Slot` and friends resolve in `s4.debug.lst` and land in the `$FF0000-$FFFFFF`
  window `debug_read` accepts (`engine.rs:62-63`, `:1542`).
- A warning worth passing on: `BusEvent` carries the raw 24-bit address unmasked, while `debug_read`
  collapses mirrors (`bus.rs:1056`, `engine.rs:1551`). aeon's `.w` short addressing sign-extends to
  `$FFxxxx`, the canonical mirror, so this should not bite — but a watch on the canonical range would
  miss an access made through the `$E0xxxx` mirror.

---

## 3. ASK 3 — cycle attribution across VBlank preemption

### Triage: **satisfied-by-in-flight** for the aggregate surface; **one real per-frame weakness of our own**

This is the ask I was told to report honestly against our own instrument, so the weakness goes under
§3.3 rather than in a footnote.

### 3.1 The defect aeon describes is the legacy instrument's, and ours is tested against it by name

Derived from `crates/oracle-core/src/profiler.rs`, not from the docs' framing:

| step | site | what happens |
|---|---|---|
| interrupt entry detected | `:883-887` | latched from the **fc = 7 acknowledge bus cycle**, level decoded from the address (`(addr >> 1) & 7`). Never from a handler address, never from a vector guess. |
| bucket + handler row opened | `:911-921` | pushes `FrameKind::Interrupt{level, frame_ssp}` **and** arms `pending_call`, so the handler also gets its own routine row nested inside the bucket. |
| cycles charged | `:612-634` | credited to the **innermost open frame only**. While a handler runs, the top frame is the handler; the preempted routine's `self_cycles` does not move. |
| **no folding into the victim** | `:719-727` | the `parent.child_cycles +=` fold is guarded on `FrameKind::Routine`. An interrupt's inclusive is deliberately not added to what it preempted. Rationale in prose at `:636-644`. |
| bucket closed | `:768-784` | `close_interrupt` matches `frame_ssp + RTE_POP == ssp_after` on the supervisor stack — correct whether the interrupt preempted user or supervisor code. |
| routine closed | `:748-763` | **value-matched, not positional**: `rposition` over the whole stack for `entry_sp + return_pop_bytes(opcode) == sp_after && supervisor == supervisor`. |
| frame boundary | `:949-1014` | every live frame is **checkpointed**, and the stack is **never torn down** (`:419-421`). |

**The legacy defect is structurally absent.** Theirs is `stack.back()` unverified over a shadow stack
*declared inside a per-frame loop* (`ControlSocket.cpp:1972`, `:1986-1996`). Ours keeps the stack across
boundaries and pops by identity. There is no positional pop anywhere in the file.

**It is pinned as an equality on a real ROM run, not argued.** `crates/oracle-core/tests/profiler.rs:1580-1769`,
`a_routines_own_cycles_are_identical_whether_or_not_an_interrupt_preempts_it`: one fixture image, run
twice, **differing by one byte of RAM** (the SR mask), asserting `cyclesSelf` and `cycles` are **exactly
equal** with VBlank live vs masked. Guarded against vacuity three ways — R's own cycles must exceed two
whole frames *derived from `MCLK_PER_FRAME / MCLK_PER_CPU_CYCLE`* (`:1631-1639`), run A must have taken
at least two VBlanks and run B zero (`:1645-1657`), and both children must have cost something. Three
mutations are recorded (`:1614-1622`), **including M1 = `self.stack.clear()` at the boundary — the
legacy defect transplanted onto our accumulator — which turns the money assertion red.**

Handler cost is attributed to the handler twice over and additively: to `interrupts.{hint,vint}` keyed by
**acknowledged level**, and to the handler's own `routines[]` row. Pinned at `tests/profiler.rs:1376-1427`
and `:1506-1551` (nested HInt inside VInt: `vint` accrues nothing while HInt is open).

The reconciliation identity — Sum of `routines[].cyclesSelfTotal` + Sum of
`interrupts[].cyclesSelfTotal` + `unattributedCycles` == `sampleCycles` — is asserted **from the wire**,
with no tolerance and no `perFrameExact` branch, at `crates/oracle-aether/tests/profiler.rs:247-297`.

**All three profiler methods are served**: `emulator/set_profiler` (`engine.rs:205`),
`emulator/get_profiler` (`:211`), `emulator/get_profiler_frames` (`:217`). None is in the 18.

### 3.2 So: should aeon migrate? Yes — and their 20.6% goes away

Their measured signature (`docs/2026-08-19-aeon-streaming-demand.md`) is a −26,163 cyc/frame hole at
2.067 frames/tick and about ±1% at 1.000 — *"when a logic tick spans a VBlank, the profiled routine that
was executing across the boundary loses cycles"*. That is precisely the per-frame-stack-rebuild defect
our M1 mutation reproduces and our gate refuses.

### 3.3 ★ Where OUR instrument is weak — three findings, reported against ourselves

**(a) `perFrame[].vintCycles` / `hintCycles` displace a boundary-straddling handler's ENTIRE cost into
the frame it returns in.** This matters because the per-frame ring is exactly the surface aeon's spike
hunt reads (`docs/2026-08-20-profiler-corpus-ab.md:704-717`). The chain, all in `profiler.rs`:

1. the ring row's cause split reads the bucket's **inclusive** figure (`:978-992`);
2. a bucket's inclusive only acquires the handler's time when the **handler frame pops** —
   `parent.child_cycles += frame.inclusive()`, the full lifetime, at `:719-726`;
3. so a boundary checkpoint taken while the handler is still running gives the bucket a delta of ~0.

Consequence: a VBlank handler starting in frame *N* and `rte`-ing in frame *N+1* reports `vintCycles`
about equal to the exception-entry cost alone for frame *N*, and **the whole multi-frame cost** for frame
*N+1* — where it can exceed that row's own `cycles`. The undivided aggregate and `perFrame[].cycles` stay
exact, so this is **displacement, not loss**. The catch-up rule is documented for *routine* inclusive
figures (`:105-108`) and is correct there; it is **not** documented on `FrameRow::vint_cycles`, whose
comment (`:358-361`) reads flatly *"What this frame's level-6 (VBlank) interrupt cost, inclusive…"*.

**Is it live? No evidence at the states already measured.** `docs/2026-08-20-profiler-corpus-ab.md:727-810`
gives 31 frames x 3 states of real aeon data: `vintCycles` is cleanly bimodal (13908/7852 at maxdiag,
21472/6212 at dense), no near-zero row and no doubled row. At 13908 cycles the handler is ~11% of a
128,000-cycle frame. **Latent, not active** — it fires only if a VBlank handler alone outlasts a frame.

A real ordering property limits the exposure: `on_frame_boundary` fires at the line-224 `Scanline` event
*before* the VInt is scheduled (`system.rs:1218` then `:1219-1220`, with `vint_offset()` > 0,
`vdp.rs:1485-1488`), so every VBlank handler begins inside the frame it is accounted to.

**(b) Non-interrupt exceptions get no frame at all — their handler's cycles land on the routine they
preempted.** The bucket path is driven *solely* by the fc = 7 acknowledge (`:883-887`). A `TRAP`, illegal
instruction, privilege violation, address/bus error, `CHK`, `TRAPV` or divide-by-zero drives no
acknowledge; its entry step carries `executed: false` (`bus.rs:91-93`) so the classifier is skipped
(`:935`), and `control_flow_of` classifies no `TRAP` encoding (`decode.rs:223-246`). Net: the trap
handler's cycles inflate the preempted routine's `cyclesSelf` and `cyclesTotal`, its `RTE` closes
nothing, and **nothing on the wire flags it**. Severity for aeon: low in steady-state gameplay, non-zero
if their engine uses `TRAP` for a debug or error path.

*Two doc defects found alongside it.* `profiler.rs:63-64` claims a `TRAP` inside a handler *"pushes its
frame lower, so its RTE … closes the trap instead"* — nothing was ever opened for the trap. The
*behaviour* is right (the bucket survives); the prose is not. `tests/profiler.rs:1442` carries the same
misreading in a comment, while its assertion is correct only *because* no trap frame exists.

**(c) The reconciliation identity is a LOSS detector, not a correctness proof.** `unattributedCycles` has
exactly one write site (`:660-663`) and one trigger (`suppressed`, set only at `:963-965` for an
interrupt in flight when the sample opens). Every charge goes to exactly one top-of-stack frame, and
every frame's accrual reaches its row. So the identity catches aeon's loss-shaped 20.6% defect and would
**not** catch a mis-keying defect like (b), where cycles are conserved but land on the wrong row. The
only mis-keying signals on the wire are `abandonedFrames` / `depthExceeded`, surfaced as a `caveat`
(`engine.rs:2782-2784`). Say this to aeon plainly rather than letting the identity oversell itself.

**Latent contract hazard, worth one line upstream and no code:** the core opens a bucket for *any*
acknowledged level (`:885` masks with `0x07`), but `get_profiler_frames` emits only `hint` (4) and `vint`
(6) (`engine.rs:2726-2729`). A bucket at another level would be silently dropped from the wire and would
**break the wire identity**. Unreachable today — `vdp.ipl()` returns only 6, 4 or 0
(`vdp.rs:1385-1393`) — because level-2/EXT is simply not modelled.

**The test gap, and it is real.** *No test in either suite puts an interrupt bucket across a mid-sample
frame boundary.* Every synthetic bucket opens and closes between two boundaries, and every ROM fixture's
VBlank handler is a bare `rte` (`testrom.rs:963`), so it cannot straddle. The core ring test does assert
`row.vint_cycles < row.cycles` (`tests/profiler.rs:894-897`) — which (a) would violate — but only against
that bare-`rte` fixture; the Aether ring test asserts no relation at all. **The cheapest next test is a
synthetic stream where an `iack` opens a bucket, a boundary lands mid-handler, and the `RTE` arrives in
the following frame.** No machine needed.

### 3.4 Migration of the three probes — verified at aeon `33cbcf5`

None of the three uses MCP. All three are raw Aether JSON-RPC over a unix socket via
`empyrean/clients/python/aether.py`'s `BusClient`, against a machine spawned by
`oracle-old/linux-port/harness/launcher.py::headless_emulator`.

**Union of methods called: 11.** Nine served; two unserved, and both are in one probe:

- served: `load_symbols`, `reset`, `run_frames`, `read_memory`, `write_memory`, `read_vram`,
  `set_profiler`, `get_profiler`, `get_profiler_frames`
- **unserved**: `emulator/write_vram` (`engine_baseline_probe.py:431`) and `emulator/run_to_scanline`
  (`:586`), both in that probe's `--sat`/DMA-scan arms.

**But method coverage is the least of it — all three break on shape.** Five hard breaks:

1. **`emulator/reset {wait, run}` → `-32602`, all three** (raster `:506`, baseline `:655`, choke `:190`).
   Our `reset` declares `params: &[]` (`engine.rs:431-435`) and dispatch refuses undeclared keys before
   the handler runs. Fix: send `{}`; re-express wait/run via `pause`/`run_frames`.
2. **`routines` is a container, not a list, all three** (raster `:562`, baseline `:771`, choke `:236`).
   We emit `{items, total, returned, truncated}`. Fix: `r["routines"]["items"]` — and read `truncated`,
   which the legacy surface never gave them.
3. **snake_case to camelCase, all three**: `frames_recorded`→`framesRecorded`, `frame_count`→`frameCount`,
   `total_cycles`→`totalCycles`, `budget_pct`→`budgetPct`.
4. **`get_profiler_frames {frames: …}` → `-32005 perFrameNotArmed`, all three.** We refuse `frames`
   unless the sample was armed `set_profiler {perFrame: true}` (`engine.rs:2603-2616`); all three arm
   with `{enabled: true}` only. `frames` is capped at `maxProfilerFrames = 120` and **refused, never
   clamped**.
5. **The launcher.** Theirs spawns `oracle_gui` under `xvfb-run` with `settings.xml`,
   `ORACLE_DETERMINISTIC=1` and an `env -C` cwd trap. Ours is
   `oracle-aether <rom.bin> [--socket PATH] [--symbols PATH] [--no-pace]` — no X, no settings file, no
   cwd trap. A replacement `headless_emulator` is small but must be written.

**Four things that get BETTER on migration — the better-than-the-floor half:**

- The `asyncio.sleep(0.4)` pairs in all three (which exist because the legacy `set_profiler` only flips a
  flag the C++ GUI main loop later drains) become dead weight — arming here is synchronous.
- Their low-24-bit `addr` masking survives untouched; no change needed.
- **They can stop refusing `interrupts.*`.** All three deliberately never read it (raster docstring
  `:4-22`, baseline `:11-16`, choke banner `:296`) because the legacy classifier buckets by handler
  *vector address* (`ControlSocket.cpp:1995`), which on their ROM sends both HBlank and VBlank into
  `else`, making `interrupts.vint` structurally `0`. **Ours keys by the acknowledged level off the fc = 7
  bus cycle, so the conflation is not expressible.** `raster_cost_probe`'s entire "read the HBlank
  trampoline's row instead" workaround becomes optional. Caveat them with §3.3(a) first.
- **`streaming_choke_probe` should be rewritten, not ported.** Its tree-by-subtraction (docstring `:9`,
  *"the only decomposition the old oracle profiler supports"*) is obsoleted by the CR-28 caller lens:
  `set_profiler {callers: true}` gives real (callee, caller) edges whose `self_cycles` and `calls`
  partition each row exactly (`profiler.rs:566-568`, pinned on the wire at
  `crates/oracle-aether/tests/profiler.rs:1219`).

### 3.5 Core readiness, cost, contract

**Core readiness: ready.** Profiler slices 1-5 plus the CR-28 caller lens are shipped and served.
**Cost:** migration is aeon's, and is a day of script work, not a parcel of ours. Our side owes: the
straddling-bucket test (small, no machine), the `perFrame[].vintCycles` doc fix (trivial), the
`profiler.rs:63-64` prose fix (trivial). **No contract CR** — except one optional upstream line noting
that only levels 4 and 6 are emitted.

---

## 4. ASK 4 — a queryable per-instruction 68000 cycle figure (sigil)

### Triage: **genuinely-new but cheap and contract-free** — and it does **nothing** for Part C

Revisions: sigil `origin/master` = **`2a02dff`** (their default branch is `master`, verified via
`origin/HEAD`, not assumed). oracle-old `d629771` (that repo has **no git remote at all**; sole local
branch `main`).

### 4.1 What sigil actually has — corrected, and the correction is consequential

`crates/sigil-isa/src/m68k_cycles.rs` (813 lines at `2a02dff`) is **not a table and is not opcode-keyed**.
It is a pure function `instr_cycles(m: Mnemonic, size: Size, ops: &[CatOp]) -> CycleCost` (`:162`) with
~175 match arms, keyed on **mnemonic + size + EA *category*** (`EaCat`, the M68000UM Table 8-2 row).
sigil is pre-encoding at that stage and **never holds an opcode word**. `CycleCost` is
`Fixed{cycles, exact}` / `Branch{taken, not_taken, exact}` / `Unmodeled` (`:89-113`).

Provenance in the header (`:1-27`) is as relayed: M68000UM §8 Tables 8-1..8-9 first, Exodus
(`oracle-old/Devices/M68000/*.h`) as the per-family cross-check, and oracle-core's SST-validated
`divs_cycles`/`divu_cycles` as a second opinion on data-dependent forms. First commit touching it:
`fed7ec1`, **2026-08-05** — the date holds.

**This is the single most consequential correction for pricing**, because it means a differential needs a
**join our tree cannot supply**: ours is keyed on the 16-bit opcode word, and **oracle has no
disassembler**. `rg 'disasm|disassemb'` over `crates/` returns only comments and symbol-file dialect
handling. There is no opcode-to-mnemonic surface anywhere.

### 4.2 Does oracle-core hold cycle costs in enumerable form? Partly — and the shape is the same as theirs

**No table exists. Cost is an accumulation over micro-ops.** Two kinds of site, enumerated:

- **Nine bus-touching arms of `MicroState::exec_one`** (`crates/oracle-core/src/m68000/microop.rs`), each
  returning `nominal + wait`: `Read` `:1611` (4), `Write` `:1640` (4), `Prefetch` `:2672` (4), `TasRmw`
  `:2789` (10), `IntAck` `:2816` (4), `MovemStore` `:2977` (4 per word), `MovemLoad` `:3028` (4 per
  word), `MovepWrite` `:3046` (4), `MovepRead` `:3062` (4). Pinned by
  `bus_wait_cycles_are_added_to_the_access_cost_in_exec_one` (`:3419-3441`).
- **`MicroOp::Internal { cycles }`** (`:1060`, executed `:2674`) — the idle/compute charge, emitted by
  the recipe builders (~95 literal sites across `decode.rs`, `ea.rs`, `microop.rs`, `exception.rs`).

Plus four **self-booking data-dependent arms**: `Mulu` (`34 + 2*popcount`, `:1718`), `Muls`
(`34 + 2*booth_transitions`, `:860`), `Divu` (`:885`), `Divs` (`:917`).

**Could a `const fn cycles_of(opcode)` be written? Not as a total pure function of the opcode word.**
`decode` takes `&Registers` and reads it five ways that change the recipe *or* its cost: the supervisor
bit (privilege gate, `decode.rs:297`); `prefetch[1]` for the MOVEM register mask (`:4840`) and the
BTST/BCHG bit number (`:1853`); `sr` for Scc/Bcc/DBcc/TRAPV outcome (`:2181, :3190, :3670, :4062`); and
`d[ccc] & 63` for register shift counts (`:1945-1953`).

**But the *shape* is pure-opcode, and it is structurally identical to sigil's own enum.** A
`fn cycle_cost_of(opcode: u16) -> CycleCost` returning `Exact(n)` / `Branch{taken, not_taken}` /
`RegCount{base, per_reg}` / `DataDependent{min, max}` is writable over the same decode structure.
**The precedent is stronger than the brief said:** beyond `return_pop_bytes`/`control_flow_of`
(`decode.rs:223-277`, exhaustively pinned at `:15489`, `:15510`, `:15532`), there is
`decode_is_total_over_the_whole_opcode_space` (`decode.rs:5722-5776`), which **already walks
`0u32..=0xFFFF` building recipes** with a fixed `Registers` and freezes a four-way class partition
(45,815 implemented / 11,529 illegal / 4,096 line-A / 4,096 line-F). **That loop is 90% of a cycle
dumper.**

### 4.3 Cheapest shape — and the contract stays shut

| shape | contract change | cost | verdict |
|---|---|---|---|
| checked-in generated table file | none | needs the generator anyway, then rots | no — strictly worse than the dumper |
| **`crates/oracle-core/examples/m68k_cycle_dump.rs`** — clone the totality loop, one row per opcode | **none** | ~150 lines | **RECOMMENDED** |
| **golden-hash test over its output** | none | ~20 lines | **recommended as its companion** — fixes the rot that kills the checked-in table |
| new bus method | **fragment in empyrean required** | fragment + handler + re-vendor + pin bookkeeping, two repos | barred unilaterally, **and pointless** — the answer is a ROM-independent constant needing no running machine |
| extend a served method | **fragment required** (all params/result objects are closed) | same cross-repo cost | no |

`crates/oracle-core/examples/` already holds 16 dumper-style binaries. It is unambiguously the house's
home for this. **Recommendation: the dumper plus the golden.** Zero contract surface, no wire invention,
no empyrean round-trip, and the artifact is a file sigil can vendor the way we vendor their schema.

**The join problem, and a better answer than the one asked for.** Don't build a disassembler. **sigil
*is* an assembler** — it already has the encoder. Have sigil emit, for each priced form, the opcode word
its encoder produces, and join on that. The join lands where the machinery already exists; our side
stays a dumb 65,536-row dump. Costs us nothing; costs them a loop over forms they already enumerate.

### 4.4 ★ Is our figure the same quantity as theirs? Not as-is — but one subtraction fixes it, and it is already on the wire

The brief was right to make this pivotal, and it was **already settled in-tree** rather than open.
`docs/2026-08-19-profiler-recon.md:43-46` says the *legacy* core's clock excludes bus/DMA stall by
construction. **Ours includes it**, deliberately — `crates/oracle-core/src/bus.rs:124-141`:

> `cycles` — *"Stall-inclusive: our clock bills bus/VDP/DMA waits to the instruction that incurred
> them."* … `stall_cycles` — *"a subset of it, never a separate quantity beside it, so
> `cycles - stall_cycles` is a well-formed subtraction."*

That is audited arm by arm, not asserted: `docs/2026-08-22-cycle-attribution-audit.md` §1.2-§1.3
re-derives the chain and enumerates every `Bus68k` arm to prove the accumulator complete, concluding
(`:103-104`) **`cycles == ideal + stall_cycles`, exactly, per step.** Only three things produce stall
(`bus.rs:131-135`): a data-port write held off by a full FIFO, a data-port read waiting for it to drain,
and the 68k-to-VDP DMA bus-hold window billed to the arming instruction.

**The transformation, stated precisely: compare `cycles − stallCycles` against sigil's ceiling, never
`cycles`.** Both terms are already emitted per row on the wire (`engine.rs:2708-2712`, `:2824-2828`).
**So the differential is NOT worthless as specified — but it would have been if run on `cycles`.**

Two caveats must travel with it:

- Neither term models **Z80 execution or 68k/Z80 bus arbitration** (a 68k access into `$A00000-$A0FFFF`
  returns wait 0, `bus.rs:1240-1247`), and there is **no DRAM refresh and no ROM/RAM wait states**.
- **`STOPPED_IDLE_SLICE = 4` per poll** for a `Stopped`/`Halted` CPU (`microop.rs:3078`) sits inside
  `cycles` but is not an ideal 68000 timing — its own doc calls it *"a progress device, not timing"*.
  **This is the one term that can push us ABOVE sigil's ceiling and fail a `<=` check for a non-defect.**
  Exclude `STOP` spinners explicitly.

### 4.5 Blind spots — where `<=` passes vacuously. Enumerated, worst first

| # | class | sigil charges | we charge | gap |
|---|---|---|---|---|
| 1 | **register-count shift/rotate** `asl/asr/lsl/lsr/rol/ror Dn,Dn` | `at_most(6 + 2*63)` = 132 word / 134 long (`m68k_cycles.rs:358-360`) | `6+2n` / `8+2n`, n = `d[ccc] & 63` (`decode.rs:1945-1953`) | a shift-by-3 is 14 vs 134 — **~9.6x under. The largest blind spot.** |
| 2 | **DIVU/DIVS overflow early-out** | 140 / 158 | flat **10** / **16-18** (`microop.rs:800-801, :824-826`) | **14x / 9x under** |
| 3 | DIVU/DIVS normal path | 140 / 158 | 76…136 / 122…156 | up to 1.8x under |
| 4 | **MULS**, any source | `at_most(70 + ea)` | `38 + 2*booth(src)`; power-of-two gives 42 | **1.7x under on the commonest real case** |
| 5 | MULU, register/memory source | `at_most(70 + ea)` | `38 + 2*popcount(src)` | ~1.3x under. *(MULU with a known immediate sigil prices exactly — not blind.)* |
| 6 | `Bcc`/`DBcc` outcome | the dearer arm | the real outcome | 10 vs 8 per site, but every not-taken branch on a hot path is a free pass |
| 7 | conditional traps that don't fire (`CHK`, `TRAPV`, div0) | the non-trap cost | the non-trap cost | comparable; a *fired* trap adds 40-50 a local walk cannot see |
| 8 | run-time recipe rewrites (address-error abort, CHK trap, div0 trap) | not modelled | rewrites the in-flight recipe entirely | unbounded, both signs |

**`MOVEM` is NOT a blind spot, contrary to the brief.** sigil carries `CatOp::RegList(u8)` and prices it
exactly (`m68k_cycles.rs:191-192`); we read the same popcount from the extension word (`decode.rs:4840`).
It is one of the *best*-comparable forms.

### 4.6 ★ The DIVS corroboration — the numbers

**sigil's header claim: verified** at `2a02dff` (`m68k_cycles.rs:9-12`, div arm `:286-293`).
**Exodus: confirmed at a committed revision** — oracle-old `d629771`,
`Devices/M68000/DIVS.h:45` gives `AddExecuteCycleCount(ExecuteTime(168, 1, 0))`, and `DIVU.h:45` gives
140 flat.

**Ours, derived from `divu_cycles` (`microop.rs:885-903`) and `divs_cycles` (`:917-941`)**, transcribed
and searched over the valid non-overflow non-div0 input space, then confirmed analytically:
DIVU = `76 + 2*n_keep + 4*n_restore` with `n_keep + n_restore <= 15`, so max `76 + 60 = 136`.
DIVS = `110 + 2*n_restore` (at most 140) plus a sign term whose maximum arm is `+2 +14`, so max **156**.

| | DIVU | DIVS |
|---|---:|---:|
| M68000UM max | 140 | **158** |
| Exodus (`oracle-old` `d629771`) | 140 | **168** |
| **oracle-core worst case** | **136** | **156** |
| oracle-core best case | 76 | 122 |

**We agree with the UM, not with Exodus.** sigil's adjudication stands and our core is a genuine second
witness for it. **The margin is 2 cycles, not the 6 sigil believes.**

**A defect this turned up in OUR documentation.** `divu_cycles`'s doc comment says *"(range 88..130)"*
(`microop.rs:882`) and `divs_cycles`'s says *"(range 126..152)"* (`:914`). **Both are understated at both
ends** — true ranges 76..136 and 122..156. sigil copied the DIVS figure straight through: their div arm
asserts *"DIVS normal path <= 152"* (`m68k_cycles.rs:289-290`); it is <= 156. Neither their
`at_most(158)` nor the adjudication changes, but **a downstream repo is carrying our stale doc comment as
a fact.** Queued as **Q-DIV-DOCRANGE** (§6). Derived by faithful transcription, not by executing the Rust
— cargo was barred — so it wants one foreground `cargo test` before the edit lands.

### 4.7 ★ Part C — the honest headline: the differential does nothing for it

**The controller's correction is right and supersedes the original framing.** A static ceiling can refute
a row reading too HIGH; it cannot refute a row reading LOW, because under a ceiling is where a ceiling
permits everything. Part C's rows read low. A differential sold as "checks our cycle numbers" would be a
vacuous gate on exactly the rows we care about.

**The narrow form does not rescue it either, and I can now say why concretely.**

*First, two corrections to where Part C even is.* It is **not** in
`docs/2026-08-22-cycle-attribution-audit.md` — that document has no Part C (its headings are §1-§9). It
is `docs/2026-08-22-shortrow-residual-measurement.md` **§8 item 1**, `:853-868`. And it is **six rows
over four routines**, not four rows: `Palette_Compose` (idle −17.0%, outside the ceiling by **242x**;
maxdiag −23.6%, **336x**), `Section_UpdateColumns` (idle −10.6%, **28x**; maxdiag −21.2%, **6x**),
`EntityWindow_Scan` (maxdiag −15.4%, **17x**), `Tile_Cache_Fill` (idle −39.9%, **13x**).

*Second, and more important: **the side reading low is theirs, not ours.*** §4.3 of that doc (`:456-485`)
hand-derives `Palette_Compose` at 180 cycles from ROM bytes and reports *"Ours: 180.0. Hand-derived: 180.
Theirs (idle): ~150."* Part C is a defect in the **legacy** instrument, and our figure is already
corroborated.

*Third — the boundability question, answered by disassembling all four.* sigil's `@budget` walk refuses
call-bearing and computed-dispatch procs (`sigil/crates/sigil-frontend-emp/src/cycle_budget.rs:27-37`;
`[cycles.computed-transfer]`, `[cycles.unbounded-loop]`, and **`[cycles.opaque-call]` — "the callee's
cost is not a local fact"**). Their gap ledger sizes it: *"235 of 419 corpus procs are structurally
boundable (no call, no back edge)."*

> **A correction on the corpus artifact.** `/home/volence/sonic_hacks/corpus-rom-d22dda85/s4.debug.lst`
> exists (286,414 B, beside `s4.debug.bin` and `PROVENANCE.md`) but **is a symbol table, not a
> disassembly** — 5,162 lines of `(0) N/ADDR : Name:`, no instruction bytes. The four routines were
> classified by parsing it for bounds and disassembling `s4.debug.bin` at those bounds with capstone
> 5.0.7 (`CS_ARCH_M68K`, `CS_MODE_M68K_000`).

| routine | entry | `bsr`/`jsr` | back edges | boundable? |
|---|---|---:|---:|---|
| `Palette_Compose` | `$007DF6` | **3** | 1 (`dbra`) plus a `bra.w` tail out | **NO** |
| `Section_UpdateColumns` | `$006E78` | **5** | 4 | **NO** |
| `EntityWindow_Scan` | `$004AA8` | **6** | 1 plus a `bra.w` tail out | **NO** |
| `Tile_Cache_Fill` | `$005D60` | **at least 1** (`jsr $A217A.l` at `$005DA4`) | yes | **NO** |

**None of the four is boundable. Every one is call-bearing; three are also loop-bearing.** sigil's walk
refuses all four before the table is ever consulted.

Worse for the narrow form: `Palette_Compose`'s *executed* path at idle takes **none** of its three `bsr`s
and **not** its `dbra` loop. The useful number is a **path** cost determined by live RAM, not a
whole-proc ceiling — a static whole-proc analysis is the wrong instrument for that shape however good
the table is.

**And the manual version of this instrument already exists in-repo and already adjudicated.** §4.2/§4.3
of the shortrow doc cost `BgAnim_Update` (154) and `Palette_Compose` (180) instruction-by-instruction
from ROM bytes against `docs/reference/Yacht.txt`, and §4.5 confirms the 154 a *third* way — from
`run_to` mclk timestamps, touching no profiler at all. §4.6: *"The hand count matches OURS on both rows,
exactly — 180 and 154 — and matches theirs on neither."* **The marginal value of sigil's table over what
§4 already did is automation and coverage, not adjudication power.**

### 4.8 sigil's self-discount, weighed

They are right to raise it, and it is smaller against **us** than against oracle-old.

- The cross-check's direction is partly knowable from their code even unenumerated: their arms are
  written as UM transcriptions with the UM cell in the comment. Where Exodus and the UM agree the
  cross-check is unfalsifiable and moot; **where they disagree, the one documented case went to the UM**,
  and our independent core lands at 156 <= 158 on the UM's side. That does not prove the cross-check
  never moved a number — but it does prove it does not win by default.
- **Against our profiler the contamination risk is structurally low**: oracle-core is ground-up Rust,
  SST-validated per family, and its cycle model is a **micro-op accumulation** rather than a per-opcode
  `ExecuteTime` constant — a different *mechanism*, not just a different transcription. Against
  `oracle-old` the concern is real; against us it is second-order.
- **The correct residual ask:** have sigil enumerate the families where the Exodus cross-check *changed*
  a number they had transcribed from the UM. Short and mechanical for them, and it converts an unbounded
  independence worry into a named exclusion set.

### 4.9 Core readiness, cost, contract

**Core readiness: partial.** The opcode-space walk exists; the cost classifier does not.
**Cost: ~150 lines plus a ~20-line golden, one small parcel. No contract CR.** A bus method would need an
empyrean fragment and is both barred unilaterally and unnecessary.

---

## 5. Proposed ordering

Ordered by *value per unit of our work*, with the cutover work aeon owns separated from the parcels we
owe. Reasons, not just a rank.

**Tier 0 — costs us nothing and unblocks the most. Do this first, today.**

**0. Send aeon the cutover packet.** Three of the four asks resolve to facts they do not currently have:
`emulator/scanlines {}` is a full-frame readback; `screenshot`/`state_hash` carry provenance; the profiler
is served, tested against their exact 20.6% signature, and buckets by acknowledged level; the five
shape-breaks in §3.4 and the composition in §2.2/§2.3. **Zero engineering, and it retires ASK 1, ASK 2
and ASK 3's headline.** Nothing else on this list beats it.

**Tier 1 — small parcels, high evidential value, no contract.**

1. **The straddling-bucket profiler test** (§3.3). It is the only *named, currently-unfalsifiable*
   weakness in an instrument a peer is about to migrate onto. Synthetic stream, no machine, no contract.
   Shipping the gate before the migration is what makes §3.3(a) a documented bound rather than a future
   incident. **First, because we are asking aeon to trust this surface this week.**
2. **The three doc fixes**, batched: `FrameRow::vint_cycles`' comment (§3.3a), the `profiler.rs:63-64`
   plus `tests/profiler.rs:1442` trap misreading (§3.3b), and **Q-DIV-DOCRANGE** (§4.6) — the last of
   which a downstream repo is already carrying as a fact. Minutes, and one is actively misinforming a
   peer.
3. **The cross-process plus pinned-golden pixel gate** (§1.6). Small, and converts "the core is
   deterministic" from an argument into an artifact aeon can cite.

**Tier 2 — one parcel each, still no contract for the first.**

4. **The opcode-space cycle dumper plus golden** (§4.3). Cheap, contract-free, and useful for the reason
   §4.7 gives — *future* coverage, not Part C. Ranked below Tier 1 precisely because its headline claim
   collapsed under checking: it does nothing for the open item that motivated it.
5. **Per-frame VDP/DMA write-volume accounting in core** (§2.7). The genuinely better instrument, and the
   one that survives aeon's refactors. Needs a contract CR for its bus surface, so it is the first item
   here that leaves this repo.

**Tier 3 — do not start without a reason.**

6. A raw-bytes/base64 option on `emulator/scanlines` (§1.6 item 3) — a rider on a fragment being opened
   anyway, never a reason to open one.
7. Serving the rest of the 18. Governed by `docs/2026-08-22-acceptance-21-survey.md`, not by these asks.
   Note only that `write_vram`, `set_layer_enabled`, `get_layer_states` and `run_to_scanline` are the
   four whose absence any of these asks noticed at all — and `set_layer_enabled`/`get_layer_states` are
   **unimplemented in core**, not merely unserved, so they are not a serving task.

**On ASK 4's placement, since I was given the argument and asked to weigh it rather than accept it:** the
"different enumeration parameter" argument for ranking it high is sound in the abstract and does not
survive §4.7. A ceiling cannot refute a low row; none of the four Part-C routines is boundable; and the
hand-derivation in §4 of the shortrow doc already did the adjudication a table would automate. It is a
good, cheap artifact for future work. It is not a Part-C instrument, and ranking it above a gate on a
weakness we are about to ship to a peer would be ranking novelty over exposure.

---

## 6. Queued items (recorded, not taken — surveying is not implementing)

For the follow-up register in `docs/OVERSEER.md`. Each is a real finding from this survey; none was fixed
here.

| id | what | where | revival condition |
|---|---|---|---|
| **Q-PROF-STRADDLE** | `perFrame[].vintCycles`/`hintCycles` displace a boundary-straddling handler's whole cost into the frame it returns in; the per-frame row can then exceed its own `cycles`. Aggregate unaffected. Latent at all measured states. | `profiler.rs:978-992` plus `:719-726` | Before aeon migrates onto `perFrame[]`. Test is synthetic and cheap (§3.3). |
| **Q-PROF-TRAPFRAME** | Non-interrupt exceptions (`TRAP`, `CHK`, privilege, address error, div0) open no frame, so the handler's cycles inflate the routine they preempted. Nothing on the wire flags it. | `profiler.rs:883-887`, `:935`; `decode.rs:223-246` | If aeon uses `TRAP` on a hot path. |
| **Q-PROF-PROSE** | `profiler.rs:63-64` and `tests/profiler.rs:1442` both claim a `TRAP` pushes a frame. Nothing is opened for it. Behaviour right, prose wrong. | as cited | Batch with Q-PROF-STRADDLE's doc fix. |
| **Q-PROF-IPLWIRE** | Core opens a bucket for any acknowledged level; `get_profiler_frames` emits only 4 and 6. Another level would be dropped from the wire and would break the wire identity. Unreachable today (`vdp.ipl()` returns 6/4/0 only). | `profiler.rs:885` vs `engine.rs:2726-2729` | If level-2/EXT is ever modelled. One contract line, no code. |
| **Q-DIV-DOCRANGE** | `divu_cycles`/`divs_cycles` doc comments state ranges 88..130 and 126..152; true ranges are **76..136** and **122..156**. **sigil has copied the DIVS figure into their own assertion** (`m68k_cycles.rs:289-290`, "<= 152"; it is <= 156). | `microop.rs:882`, `:914` | Immediately — a peer is carrying it as a fact. Wants one foreground `cargo test` first (derived by transcription; cargo was barred). |
| **Q-PNG-STALEREF** | `crates/oracle-aether/src/png.rs:23` points at `tests/png_roundtrip.rs`, which **does not exist** (`git ls-files` shows only `src/png.rs`). The round trip was really performed and its result is locked in as a byte-literal golden at `:421-444` — but a reader chasing `:23` finds nothing and could reasonably conclude the encoder is unverified. | `png.rs:23` | Next time `png.rs` is opened. One line. |
| **Q-AEON-STALEPATH** | Five aeon tools point at `oracle-next/target/release/oracle-aether`, the **pre-rename** path; only `tick_variance_probe.py` uses the current `oracle/`. Theirs to fix, ours to report. | aeon `33cbcf5`: `boot_override_gate.py:76`, `hblank_window_sweep.py:139`, `sh_probe.py:16`, `vsplit_landing_gate.py:99`, `warp_mailbox_gate.py:116` | Send with the cutover packet. |

---

## 7. What must go back to aeon and sigil

**To aeon:**

1. The three capture protocols that failed their controls — were any run against *our* server? If so we
   want the failing call verbatim; nothing in §1 would explain it, and that would be a real defect.
2. Do they intend to read `perFrame[].vintCycles`? If yes, Q-PROF-STRADDLE's bound applies and we should
   ship the gate first.
3. Does their engine take `TRAP` (or `CHK`/div0) on any profiled path? That decides Q-PROF-TRAPFRAME's
   severity, and only they can answer it.
4. The per-queue slot capacities (`slots_c`/`slots_i`/`slots_d`) for the ASK 2 watches.
5. Confirmation that they want `streaming_choke_probe` **rewritten** on the caller lens rather than
   ported — more work than a port, and a strictly better instrument.

**To sigil:**

6. **The Exodus-cross-check exclusion set** — which families did the cross-check *move* off the UM
   transcription? Short and mechanical for them; converts an unbounded independence worry into a named
   list (§4.8).
7. Will they emit the encoder's opcode word per priced form, so the join lands on their side? That is the
   difference between a cheap dumper and us building a disassembler we do not have (§4.3).
8. **Q-DIV-DOCRANGE, immediately**: their `<= 152` DIVS assertion is ours-stale; the true bound is
   `<= 156`. Their `at_most(158)` and their adjudication are unaffected.

---

## 8. Where the briefs were wrong

Recorded because this is the survey's most reliable output, and several of these would have propagated.

1. **"~37 served, 18 unserved."** 18 is right; served is **40**. The step trio moved both halves; the
   brief moved one. (§0.2)
2. **"aeon's probes talk to the legacy server today."** Half true. Their **cost/profiler** probes do;
   their **pixel and streaming** probes already drive our Rust server headlessly — seven of them. The
   cutover is partial and in progress. (§0.3)
3. **"whether anything about running it headless is actually unsolved."** Nothing is, and the proof is
   aeon's own `subprocess.Popen([SERVER, rom, "--socket", sock, "--no-pace"])`. (§0.3)
4. **"breakpoints plus `wait_for_break` to stop at the drain."** Both unserved;
   `capabilities.breakpoints` is hard-`false`. The substitute — `run_to {symbol}` — is *better*, and
   nobody had noticed `run_to` takes a symbol. (§2.5)
5. **"a hit cap that would silently drop hits."** There is a cap (4096) but the drop is not silent on any
   of three axes. (§2.4)
6. **"`F-TICK-BOUNDARY-DIVERGENCE` is relevant prior art on attribution."** It is not. It records that
   oracle-old runs 26 logic ticks where we run 29 over a 31-frame window — a machine-timing
   disagreement, not an accounting one, and marked "Unresolved, not urgent". It neither supports nor
   undermines the profiler verdict.
7. **"the ~20% loss."** 20.6%, and its mechanism is **two** defects with two signatures: the per-frame
   shadow-stack *rebuild* (`ControlSocket.cpp:1972`) produces the 20.6% loss; the *positional pop*
   (`:1986-1996`) produces the separate "rows read LOW by 13x-242x" family.
8. **"CRAM reads are frame-latched" does not describe this repo.** `Vdp::cram()` is live and this was
   settled on 2026-08-17. The real trap here has a different shape: the renderer uses a per-line CRAM
   snapshot plus a sub-line write journal while `read_cram` reads live CRAM — and, sharper still,
   `pixel_attribution` and `screenshot`/`scanlines` read different things by design. (§1.2, §1.3)
9. **"a witness buffer that turned out uniform."** The analogous hazard here is not a uniform buffer but
   the silent `stateRender` fallback — a well-formed, schema-valid, frame-blind picture. The flag that
   catches it already exists. (§1.3)
10. **"assess whether ours can be made to fail on purpose."** Already done and shipped: two poison
    fixtures and a landing band derived from the fixture's own instruction costs. (§1.4)
11. **sigil's artifact is not "a static per-instruction cycle table."** It is a pure function keyed on
    mnemonic + size + EA category, holding no opcode. This is the most consequential correction in the
    ASK 4 pricing — it means the differential needs a join our tree cannot supply. (§4.1)
12. **Part C is in the wrong document and is six rows, not four** —
    `2026-08-22-shortrow-residual-measurement.md` §8:853-868, six rows over four routines. **And the side
    reading low is theirs, not ours.** (§4.7)
13. **`MOVEM` is not a differential blind spot** — it is one of the best-comparable forms. (§4.5)
14. **"only genuinely-new generates work across the fence."** ASK 4 is genuinely-new and generates *no*
    cross-fence work: a dumper in `examples/` needs no contract change at all. The triage vocabulary's
    implicit "new implies CR" does not hold. (§4.3)
15. **The corpus `.lst` is a symbol table, not a disassembly.** Anything asking "is this routine
    straight-line?" needs a real disassembler over `s4.debug.bin`, not that file. (§4.7)
16. **Minor:** oracle-old has no git remote (sole local branch `main`), and sigil's default branch is
    `master`. The brief's warning not to assume `master` was right in spirit and inverted in this pair.

---

## 9. BLOCKED / TAGGED — what this survey could not settle

**Nothing is BLOCKED in the sense of "unanswerable".** Every triage verdict above is derived from source.
Two classes of confirmation were out of reach by constraint and are tagged for the foreground lane:

- **Cargo was barred** (a peer agent held the lane), so **not one test was executed**. Every "pinned"
  claim is a reading of assertion text, not of a green run. Wanted: `cargo test -p oracle-core --test profiler`,
  `-p oracle-aether --test profiler`, `-p oracle-aether --test scanlines`, and one run to confirm
  Q-DIV-DOCRANGE's derived ranges before that doc edit lands.
- **The emulator MCP was barred** (it deadlocks from background agents, and it reaches the legacy server
  anyway). So nothing was run on a machine. Wanted, all for ASK 2's composition: that
  `DMA_Critical_Slot` and friends resolve in `s4.debug.lst` and land in the window `debug_read` accepts;
  the actual per-frame hit volume against the 4096 ring at a streaming-active state; and that
  `run_to {symbol:"Process_DMA_Critical"}` lands once per frame at the states aeon cares about.
- **§3.3(a) is derived, not witnessed.** Confirming the displacement needs either a synthetic stream
  (cheap, no machine — the recommended route) or a ROM whose VBlank handler outruns a frame.
- **empyrean was not read directly** at a committed revision for the contract-location claim; that rests
  on `crates/oracle-aether/tests/contract/PROVENANCE.md`, which records the upstream path, SHA, blob and
  byte count. One firsthand read of empyrean `origin/main` would close the link.
