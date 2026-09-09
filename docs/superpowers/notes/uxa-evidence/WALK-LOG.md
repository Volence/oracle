# UXa — task walk, running log

Seat: UXa (task walk), Roster C. Tree: `/home/volence/sonic_hacks/oracle-uxa`, branch `seat/uxa`,
built from `cb21f43`. Display: `:90` (Xvfb, `launch.sh display-start`, geometry read back from
inside: 1280x960 depth=24). Client: only `tools/uxrig/drive.py` over its own connection to `:90`,
plus the binaries in **this** worktree's `target/release/`. No MCP, no `/dev/uinput`.

Every time below is UTC wall-clock from `date -u`.

## Timeline

| time (UTC) | what |
|---|---|
| 07:39:20 | walk starts; charter + handover read |
| 07:39:39 | worktree created. **`s4.debug.bin` already absent** — first check of the run |
| 07:39:54 | `cargo build --release -p oracle-player -p oracle-frontend` started |
| 07:40:26 | display `:90` up |
| 07:44:43 | build finished, 4m49s, rc=0 |
| 07:45:0x | ROM still absent; three `./build.sh` processes live in the aeon tree |
| 07:45:1x | no-ROM probes run against both binaries while waiting |

## Job 1 — "get a game running and see it", from the README alone

### The README does not carry a newcomer to a loaded ROM. Two independent reasons.

**(a) The README never tells you where a ROM comes from.** Both run recipes are
`cargo run ... -- <rom.bin>` with `<rom.bin>` a placeholder. `grep -niE rom README.md` returns 13
lines; the only concrete artifact named is "Aeon's debug ROM" (line 95), with no path, no repo, and
no instruction to build it. A newcomer with a clean checkout cannot execute step one.

**(b) The README does not know the debug window exists.** `grep -n "oracle-player" README.md` →
**no match, rc=1**. The Layout section says *"Four crates in one workspace"* and names
`oracle-core`, `oracle-aether`, `oracle-frontend`, `oracle-replay`. `ls -1 crates/` shows **six**:
those four plus `oracle-panels-spike` and **`oracle-player`**. `oracle-player` is the entire debug
surface — Registers, Memory, Objects, Screen, Pacing — i.e. the window that does jobs 2 through 5.
A newcomer reading only the front door does not learn that it exists, and would attempt every
debugging job in the wrong window.

Window: this is a repo-front-door finding that lands **against `oracle-player`** (the window that is
missing), discovered before either window was launched.

## Job 1 sidebar — asking either window for help

Evidence: `help-frontend.txt`, `help-player.txt` (verbatim, with exit codes).

| invocation | `oracle-frontend` (game window) | `oracle-player` (debug tabs) |
|---|---|---|
| `--help` | ``error: unknown flag `--help` `` then usage, **EXIT=2** | full usage + 5 paragraphs of prose, EXIT=64 |
| `-h` | `cannot read ROM -h: No such file or directory (os error 2)`, **EXIT=1** | same full usage, EXIT=64 |
| no args | `error: missing <rom.bin>` + usage, EXIT=2 | `--rom is required` + usage, EXIT=64 |

The two windows disagree about the same fact — *how do I ask this program what it takes?* — and the
game window's answer to `-h` is actively misleading: it reports a **missing ROM file called `-h`**,
never mentions that `-h` is not a flag, and prints no usage at all. `--help` is called an *error*
by the one window and is the documented way in by the other.
