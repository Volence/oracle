# oracle

A **ground-up Rust Sega Genesis / Mega Drive emulation core**, plus **Aether** — a JSON-RPC 2.0
control surface over an `AF_UNIX` socket that lets agents and tools drive and inspect the machine.

It replaces the legacy C++ engine (an Exodus port) that lives in the sibling checkout
**`../oracle-old/`**. As of 2026-08-30 this repo is the **suite default** (owner ruling: feature
gaps become asks on this repo, not reasons to fall back); the legacy port stays beside it as the
differential reference. Read the next section before you assume anything about what is in
production.

## The `mcp__oracle__*` MCP tools reach THIS repo by default

The MCP server registered as `oracle` is still the Python shim in the legacy repo — its command is
`/home/volence/sonic_hacks/oracle-old/linux-port/mcp/oracle-mcp` — but **by default it spawns this
repo's `target/release/oracle-aether`** on a private socket and drives it. It attaches to an
already-running server (for example the legacy C++ `oracle_gui`) only when `ORACLE_SOCKET` (or the
legacy `EXODUS_SOCKET`) is set. So `mcp__oracle__emulator_*` calls exercise this core unless you
deliberately point them elsewhere.

⚠ **And if you do point them elsewhere, know what you are pointing them at.** The legacy C++ server
**silently defaults parameters it does not recognise** — ask it for a key it has never heard of and
its accessor hands back the default and tells nobody. So a stale, misspelled or newly-renamed call
against `oracle_gui` returns a plausible answer rather than an error, and a gate built on one can
observe for thirty seconds while reporting that it observed for a hundred and twenty. This server
does the opposite on purpose: an unknown method is `-32601` by name, and an unknown or malformed
parameter is `-32602` naming the key, refused at the dispatch choke *before* the handler runs. That
asymmetry is the whole reason the cutover was sequenced onto this implementation rather than away
from it — **a missing capability that returns something is far worse than one that refuses.**

The Python MCP client itself has never been ported into this repo; the cross-checking test
`crates/oracle-aether/tests/mcp_tool_sweep.rs` reads `oracle-old/linux-port/mcp/oracle_mcp.py` off
disk precisely because it is a foreign artifact.

**Where the Rust server actually stands.** The shared bus contract
(`empyrean/contract/protocol.md`, vendored here as
`crates/oracle-aether/tests/contract/bus-protocol.schema.json`) catalogues a set of request methods;
this server serves all but the eight below, and that list *is* the acceptance contract for the
cutover.

**No total is printed here on purpose.** Earlier revisions of this section carried one, it went
stale three landings later while every assertion around it stayed green, and a consumer elsewhere in
this suite once pinned a published method count as a freshness check — so the guard written to
detect staleness became the stale thing and rejected every correct binary. A count changes for
reasons unrelated to the question anyone is actually asking. Derive it when you need it:

```sh
# served, from the one table that is simultaneously the dispatch table and the advertised list
grep -oE 'name: "[A-Za-z0-9_/]+"' crates/oracle-aether/src/engine.rs | sort -u | wc -l
```

Note the character class: `[a-z_]` silently drops `emulator/romReloaded` behind the capital R and
returns a smaller number with no error, which is how two independent derivations in this repo once
agreed with each other and were both wrong.

| Group | Unserved methods |
|---|---|
| Sound (`capabilities.vgm: false`) | `audio_spectrum`, `get_channel_states`, `set_channel_enabled`, `vgm_start`, `vgm_status`, `vgm_stop` |
| Other | `log_clear`, `ping` |

All names are `emulator/`-prefixed on the wire. The list is pinned as
`SCHEMATIZED_NOT_ADVERTISED` in `crates/oracle-aether/tests/schema_conformance.rs`, which fails if
a method enters or leaves it without a deliberate edit. The served list is
`engine::METHODS` in `crates/oracle-aether/src/engine.rs` — one table that is simultaneously the
dispatch table and the `initialize` reply's advertised methods, so the two cannot drift.

The remaining `false` capability flag is about **the bus surface, not the core**, and the
distinction matters when reading it:

- `"vgm": false` — the core *has* VGM capture (`crates/oracle-core/src/vgm.rs`, and the
  `vgm_capture` example). What is missing is the bus methods to start and stop it.

`"z80"` and `"breakpoints"` are **served surfaces now**: the Z80 window (`z80_read`/`z80_write`)
sits over a core Z80 (`crates/oracle-core/src/z80/`) with the whole documented instruction set
graded against SingleStepTests/z80 (only the undocumented opcodes remain), and a real breakpoint
engine (`crates/oracle-aether/src/breakpoints.rs`) backs the five served breakpoint methods.
Watchpoints are a separate, working surface (`capabilities.watchpoints`, four served methods).

## Layout

Four crates in one workspace (`Cargo.toml`):

- **`crates/oracle-core`** — the emulator. Deterministic and I/O-free by charter; one dependency
  (`bincode`), `#![forbid(unsafe_code)]`, no threads. One `System` owns all memory and chips and a
  single `Scheduler` that holds the sole master clock and one seeded RNG. 68000, Z80, VDP, YM2612,
  SN76489, I/O, symbols, watchpoints, profiler.
- **`crates/oracle-aether`** — the Aether server. JSON-RPC 2.0 as NDJSON over an `AF_UNIX` socket
  at mode 0600, with an `initialize`/`initialized` handshake and server-pushed events. Sockets,
  threads and JSON live here so the core's charter stays intact. Two arrangements over one engine:
  `server` (the bus owns the machine on its own thread) and `host` (something else owns the run
  loop and pumps the bus).
- **`crates/oracle-frontend`** — a windowed player (minifb) over the same core: keyboard + gamepad,
  audio, save states, a command palette, and debug lenses. Can host the Aether bus in-process with
  `--aether`.
- **`crates/oracle-replay`** — `replay_runner`, a headless gate binary that boots Aeon's debug ROM,
  replays a recorded input fixture, and exits PASS / DESYNC / FAULT / TIMEOUT.

## Build and test

CI (`.github/workflows/ci.yml`) pins Rust **1.96.0** and runs exactly this, determinism first:

```sh
# The gating job — nothing else runs unless determinism holds.
cargo test -p oracle-core --test determinism_gate --test proptests -- --nocapture

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Pinned external test data. Gitignored; the runners skip cleanly (and loudly) when absent,
# except under CI where a guard test turns a missing corpus into a hard failure.
./tools/fetch-tests.sh        # SingleStepTests/680x0
./tools/fetch-z80-tests.sh    # SingleStepTests/z80
./tools/fetch-testroms.sh     # Mega Drive test-ROM corpus (conformance_roms.rs)

cargo test --workspace        # includes the SST sweep; it takes minutes, it is not hung
```

Run the bus server, or the player:

```sh
cargo run -p oracle-aether -- <rom.bin> [--socket PATH] [--symbols PATH] [--no-pace]
cargo run --release -p oracle-frontend -- <rom.bin> [--scale N] [--aether]
```

With `--socket` omitted the path resolves `$ORACLE_SOCKET` → `$EXODUS_SOCKET` →
`$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`. Symbols are opt-in by presence: a `<rom>.lst`
beside the ROM is loaded if it exists, and **refused** if it does not bind to the image.

## What is and isn't true today

- **Works:** the 68000 core (graded case-by-case against SingleStepTests/680x0 through both a
  run-to-completion and a cycle-stepped driver, including the per-cycle bus-transaction stream);
  the Z80's documented instruction set; VDP planes/sprites/scroll/DMA and a render path; FM and PSG
  synthesis with real-time audio in the player; snapshot/restore; watchpoints; breakpoints; the
  CPU profiler; and 56 of the 64 bus methods.
- **Not done:** the 8 bus methods above (the sound/VGM surface plus `log_clear` and `ping`); a
  native MCP in this repo (the shim in `oracle-old/` drives this server by default); the
  undocumented Z80 opcodes; six-button pads (`capabilities.sixButtonPad: false` — refused
  rather than silently ignored); batch requests; object decoders; PAL timing (the core is NTSC-only
  and says so in every reply's `timingBasis`).
- **Deliberately not a pass/fail gate:** `crates/oracle-core/tests/conformance_roms.rs` boots a
  corpus of public test ROMs and compares the *whole scorecard* against a pinned baseline. Several
  ROMs fail today for reasons written up in `docs/2026-07-25-testrom-conformance.md`. The baseline
  is a photograph of the present, not a claim of correctness; the test fires on movement in either
  direction.
- **Accuracy is an asymptote, not a launch bar.** `CHARTER.md` sets the target at
  MVP-debuggable, explicitly not "passes VDPFIFOTesting".

Do not quote a built binary's size or a frame count from a doc — check the artifact.

## Where the real documentation lives

`README.md` is the front door and nothing more. The substance is:

- **`CHARTER.md`** — why this exists, what was chosen over what, and the honest risk list. Note
  that it still uses the old dev name `oracle-next` throughout; this repo is now `oracle/` and the
  legacy C++ one is `oracle-old/`.
- **`docs/OVERSEER.md`** — the working queue and the session boot prompt. **Start here** for what
  is actually being worked on.
- **`docs/`** — dated arc records: recon documents, designs, plans, change requests, adjudicated
  rulings, and handoffs, one file per push, newest names carrying the newest state.
- **`docs/decisions/`** and **`docs/plans/`** — the standing policies and the per-slice build record.
- **`crates/oracle-aether/tests/contract/`** — the vendored wire schema and its provenance.

The bus protocol itself is **not** owned here. It lives in the `empyrean` repo
(`empyrean/contract/protocol.md`) and is normative: this server conforms to it, and any place it
could not is filed as a change request rather than taken silently.
