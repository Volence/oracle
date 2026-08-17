# The MCP reaches every method now, and the gap is measured rather than remembered

Follows `docs/2026-08-15-mcp-as-aether-client-validation.md`, which reported **9 Aether methods no MCP
tool could reach** and proposed three tool-table rows for the worst of it. Owner ruled: close the whole
gap, and make `list_tools` negotiate rather than guess.

| repo | commit | state |
|---|---|---|
| `oracle` (the MCP) | see below | committed — **this repo has no git remote, so it cannot be pushed** |
| `oracle-next` | this doc | pushed |

## The gap was 12, not 9 — and three of them were one day old

Measured against a live server rather than against the report: 28 advertised methods, 50 tool rows,
16 shared.

```
emulator/checkpoint          emulator/read              emulator/watchpoint_clear
emulator/checkpoint_drop     emulator/restore           emulator/watchpoint_hits
emulator/checkpoint_list     emulator/run_frames        emulator/watchpoint_list
emulator/pixel_attribution   emulator/sprites
emulator/play_input
```

The report's nine, plus `sprites`, `play_input` and `read` — **all three shipped the night before, each
with a contract row, a schema fragment and a mutation-verified test suite, and no client able to call
any of them.** That is the same defect the report named, committed again one session later, which is the
argument for measuring it instead of noticing it.

**None of the twelve were unreachable by design.** The deliberate emptiness runs the other way: 34 tools
our server does not advertise, a third of them the interactive-debugger family that is ruled dead on
purpose.

## What shipped

**Twelve tool rows** in `oracle/linux-port/mcp/oracle_mcp.py`, each schema written from its §6 contract
row rather than from the server's behaviour — the discipline the last run's two client bugs were caused
by ignoring. Addresses are hex strings (D9), `port` is 0 or 1, `play_input`'s rows carry the half-open
`[start, end)` shape and the description states the property that makes it worth having (*the pad at
frame N is a pure function of `rows`*), because a normative guarantee a client cannot see is a guarantee
the model will not rely on.

**`list_tools` now filters on the handshake.** One table serves two implementations; unfiltered, 34 of 50
tools could not work against oracle-next, and after this change 11 of 62 cannot work against Oracle. The
server has advertised its live method set since D4 and the Python client has exposed it as
`BusClient.methods` all along — nothing new had to be built, only asked.

**It fails open, deliberately.** An MCP client asks for the tool list once, often before the emulator is
running. A filter that failed closed would hand the model an empty toolbox in exactly that case, so an
unreachable bus means *show everything* and let a real call take a clean `-32601`.

**`coverage_check.py`**, so this is a check and not a memory. It asks the live server what it advertises,
diffs both directions, and exits 1 when a method has no row — the *"a harness must report its own
coverage or green reads as complete"* lesson, applied to the client side for the first time.

## Verified

Against `s4.debug.bin` on a clean boot: **`MISSING TOOL ROWS (0)`, `list_tools` returned 28 for 28, and
all 28 tools answered through the real `call_tool` entry point** with the parameters their own schemas
declare. EXIT=0.

Mutation-verified, one line each:

- Delete the `sprites` row → `MISSING TOOL ROWS (1)`, FAIL. *(caught)*
- Remove the filter from `list_tools` → 62 offered, 34 of them unusable here, FAIL. *(caught)*
- Point at a dead socket → `served_methods() -> None`, all 62 tools offered. *(fail-open holds)*

**Oracle cannot be run here, so its side was checked statically** against `Handlers()` in
`ControlSocket.cpp`: it advertises 53 methods, and **the filter hides none of the 50 pre-existing rows
from it** — the change is a no-op for Oracle except that it stops offering the 11 oracle-next-only rows.
Two Oracle methods still have no row (`ping`, transport; `debug_arbiter`, Oracle-internal); neither is
model-facing capability.

## Three things the exercise turned up

**1. `run_frames` was missing from both servers.** Oracle has served it all along and never had a tool.
The most basic bounded advance on the machine, unreachable from the MCP against either implementation,
for as long as both have existed.

**2. Filtering removes a footgun nobody had noticed.** Oracle's dispatcher still has a legacy `read` op
that it advertises canonically as `read_memory`. An unfiltered `emulator_read` pointed at Oracle would
therefore have *worked* — reading bus memory while silently ignoring `space: "vram"`. The handshake
filter never offers it there.

**3. The probe's own three failures were the probe's, again — 3 for 3 with the last run.** `load_symbols`
called with no `path`; `run_frames` and `run_to` called on a running machine because the sweep visits
tools in name order and `resume` sorts just before them. Fixed in the harness (`PAUSE_FIRST`, and
`load_symbols` driven with the server's own `symbolsPath`), not in the server. **Every failure a
hand-written probe reports is the probe's until proven otherwise.**

## Not done, deliberately

`emulator_watchpoint_add`'s row still exposes only Oracle's four parameters (`addr`, `symbol`, `read`,
`write`). The contract has carried `space`, `len`, `mode`, `censusKey`, `stopAfter` and `label` since
CR-11/12, so **from the MCP a watch can now be read but still cannot be armed on VRAM, CRAM or VSRAM, or
armed in census mode.** That is a *reachable* method with an under-declared schema — a different defect
from the one this closes, and it wants a decision about widening a row two servers share rather than a
reflex. Registered here so it is a choice and not an oversight.
