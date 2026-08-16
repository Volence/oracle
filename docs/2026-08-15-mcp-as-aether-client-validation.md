# The MCP as a client of Aether — the validation run (2026-08-15)

Ranked item 1 of `docs/2026-08-15-handoff-conformance-and-item19.md` §7. The owner ruled the open
sequencing call **MCP first, scoped as validation rather than product** — the unified-read collapse
(capability 1) waits on what this run learns.

## The framing was wrong, and cheaper than recorded

The queue said *"port the MCP onto Aether."* **The MCP was already ported.** `oracle_mcp.py` has spoken
Aether since D10 landed: it imports `BusClient` from `empyrean/clients/python/aether.py`, and its own
docstring says *"The MCP is now a client of the bus, not its own protocol (protocol.md D10)."* Its 50 tools
map mechanically — tool `emulator_foo` → method `emulator/foo`, in `call_tool`, with no per-tool glue.

So there was nothing to port. What had never happened is the thing D10 was actually for: **pointing that
client at the second implementation.** The MCP has only ever talked to Oracle. The exercise is therefore not
a port but an **A/B of two servers against one real, independently-written client** — and it cost an
afternoon rather than the arc the queue implied.

## What was run

`oracle-aether` serving `aeon/s4.bin` (697,793 bytes, 2,178 symbols bound from `s4.lst`) on a private
socket, driven by:

1. the unmodified reference `BusClient`, over the MCP's own 50-tool surface (pass 1, then pass 2 with the
   full parameter set each tool's `inputSchema` declares, from a paused machine); and
2. **the real `oracle_mcp` module itself**, imported as-is and driven through `call_tool` — the actual
   dispatch path, not a re-implementation of it.

Socket note: the session scratchpad path exceeds `SUN_LEN`, so the socket lives under `$XDG_RUNTIME_DIR`.
Our server refused it with the right error rather than truncating, which is the correct behaviour and is
recorded here only because the next person will hit it too.

## Result: the real MCP drives oracle-next end-to-end

**MEASURED.** Handshake clean (`serverName: "oracle-next"`, protocol 1, 25 methods advertised). Through the
real `call_tool`: `read_vram`, `read_memory` by symbol, `registers`, `lookup_symbol`, `press`, `screenshot`,
`status` all return sane values against a booted ROM. Of the 16 tools whose method our server advertises,
**15 succeed with the parameters the MCP's own schema declares**, including the one-of `addr`/`symbol`
forms, `$`-prefixed hex, `includeFramebuffer`, a caller-supplied screenshot path, `load_symbols` on the real
listing, and `reload_rom`.

That is the D10 claim demonstrated rather than asserted. It also found two client bugs.

## Divergence 1 — `read_vram.addr` is declared an integer (CONFIRMED, fixed client-side)

**MEASURED.** The MCP declares `read_vram.addr` as `{"type": "integer"}`. Our server refuses a numeric
address per D9 — `-32602`, *"`addr` must be a hex string like \"0x00FF0000\" (D9), not a number"*. Every
`emulator_read_vram` call with an explicit address fails against oracle-next.

**MEASURED, and bounded.** A sweep of all 50 tools found **exactly two** of the twelve address-taking tools
declaring `addr` as an integer — `read_vram` and `write_vram`, both VDP-space. The other ten are hex
strings, `read_memory` among them. The MCP's own surface was internally inconsistent.

**MEASURED (by code read, both sides).** Oracle absorbed it silently: `JsonObj::getInt` accepts a number
*and* `"0x…"` *and* `"$…"`, so both spellings worked there and the inconsistency never surfaced. That also
makes the fix free: **hex strings are valid against Oracle too**, so correcting the two declarations breaks
nothing. (Oracle's `getInt` additionally swallows a malformed address into the default `0` via
`catch (...) { return d; }` — a bad address silently reads VRAM 0 and reports success. Ours refuses. Noted
as corroboration, not as work.)

**Fixed** in `oracle/linux-port/mcp/oracle_mcp.py`: both declarations are now `string`. Verified no
integer-typed address param remains.

## Divergence 2 — the screenshot the model "sees" was a mislabelled PPM (CONFIRMED, fixed client-side)

**MEASURED, and the more serious of the two.** `call_tool` special-cases `screenshot` to return an MCP
`ImageContent` block so the model can look at the frame. It read `path`, base64'd the bytes, and hardcoded
`mimeType="image/png"`.

Our server writes **PPM** — `format: "ppm"` in the result, `P6` magic confirmed by `file`. So against
oracle-next the model received 215 KB of PPM labelled `image/png`: undecodable, and *presented as a frame
it can see*. Passing `path: ".../oracle-snap.png"` made it worse — a file whose name lies about its
contents (verified: `P6\n320 224\n255\n` inside `oracle-snap.png`).

**MEASURED.** Oracle writes a genuine `.png` and **emits no `format` field at all**. The client's hardcode
was therefore *true by accident* against the only server it had ever met. This is the exact failure mode
D10 exists to prevent — a client encoding one implementation's incidental behaviour instead of the
contract's declared field — and it took a second implementation to expose it. **Our server was conformant
throughout; it published `format` and the client ignored it.**

**Fixed** client-side: `call_tool` now honours `format`, maps it to a real mime type, and for anything not
inline-displayable returns text naming the path and format instead of a corrupt image block.

Both branches verified, and the check discriminates:

| reply shape | blocks returned | mime |
|---|---|---|
| Oracle's (no `format`, real PNG) | `['image', 'text']` | `image/png` |
| oracle-next's (`format: "ppm"`) | `['text']` | — |

The Oracle branch is the discriminating half: it proves the fix reports honestly *without* regressing the
server that was already working.

### ☐ OWNER CALL — this leaves the model unable to see the screen on oracle-next

Honest text is better than a corrupt image, but it is not a frame. A model driving oracle-next through the
MCP now cannot look at the picture, which for this project's actual usage is close to the whole point.
Three options, none taken:

1. **Emit PNG from `emulator/screenshot`.** No PNG dependency exists anywhere in the workspace, and
   `oracle-aether`'s runtime deps are deliberately `oracle-core` + `serde_json`. A dependency-free encoder
   using stored deflate blocks is ~50 lines but compresses nothing (a 215 KB frame stays 215 KB); a real
   fixed-Huffman deflate is ~150; the `png` crate is one line and a policy change.
2. **Convert client-side**, which puts an image dependency in the MCP instead.
3. **Leave it**, and view frames through the player window.

Recommendation: **(1) with the `png` crate**, if the dependency policy allows — it is the only option that
makes the contract's `format` field mean something and keeps every client thin. Not done, because it is
product work and this exercise was scoped to validation.

## The gap, in both directions

**MEASURED.** 50 MCP tools; 25 advertised methods; **16 shared**.

### 9 Aether methods no MCP tool can reach

`run_frames`, `checkpoint`, `restore`, `checkpoint_list`, `checkpoint_drop`, `watchpoint_clear`,
`watchpoint_list`, `watchpoint_hits`, `pixel_attribution`.

**This is the finding worth acting on, and it is embarrassing in a specific way: our newest and most-worked
surfaces have no client.** The watchpoint surface shipped this session across CR-11/CR-12 — and from the
MCP a watch can be **armed and never read**. `watchpoint_add` is reachable; `_list`, `_hits` and `_clear`
are not. An instrument you can arm but not read is not an instrument. Checkpoints (four methods, the
string-id work of §8 item 16) are likewise unreachable, as is `pixel_attribution` (CR-10, adopted today).

Three MCP tool-table rows would make the watchpoint surface usable end-to-end. That is table entries, not
design — but it is *product*, so it is proposed here rather than done.

### 34 MCP tools our server does not advertise

Classified — and this list is **explicitly not a build backlog**, because a third of it is already ruled
dead:

- **KEEP-DEAD (8), do not re-fund**: `breakpoint_add`/`_clear`/`_list`, `call_stack`, `step`, `step_out`,
  `step_over`, `wait_for_break`. The interactive-debugger family, ruled dead with reasons; the ledger's
  measured record is that its absence forced better answers.
- **Write ops (4), adjacent to a KEEP-DEAD entry and needing a ruling, not a build**: `write_memory`,
  `write_vram`, `write_cram`, `z80_write`. The dead entry names "a register-write op"; whether it covers
  memory writes is a ruling nobody has made.
- **Backed by shipped core capability, exposable (13)**: `read_cram`, `memory_hash`, `z80_read`,
  `z80_registers`, `reset`, `run_to_scanline`, `vgm_start`/`_stop`/`_status`, `audio_spectrum`,
  `get_channel_states`/`set_channel_enabled`, `get_layer_states`/`set_layer_enabled`. Every one of these
  sits on something already built and tested (Z80 core, VgmLogger, the synth, per-scanline capture).
  **`reset` is the conspicuous absence** — the most basic control op on the machine, missing from a
  25-method control surface.
- **Game-specific (3), and they may not belong on this bus at all**: `object_list`, `object_slot`,
  `player_state`. These read Sonic's object RAM at fixed offsets. Aether is game-agnostic by construction,
  so the right home is a client that knows the game, not the bus. **A real architectural finding** — worth
  a decision before anyone ports them by reflex.
- **Host-specific (4)**: `log_tail`, `log_clear`, `get_profiler`/`_frames`, `set_profiler`. Tied to
  Oracle's GUI and its profiler; no obvious meaning for a headless core.

### What this says about capability 1 (the unified read)

**Evidence, not a decision.** The exposable-13 contains `read_cram`, `z80_read` and `memory_hash` — three
more read shapes on top of the two we already ship. Adding them one method at a time is how a six-method
read surface became a collapse candidate in the first place. The sequencing tension the owner ruled on is
therefore live in a concrete way: **the collapse now has a client that would exercise it**, which is the
condition its ranking was waiting for.

## Lessons this run re-confirmed

1. **A hand-written probe measures its own reach — again.** Pass 1 reported "0 server defects" and was
   worthless: it never paused the machine (3 spurious `-32005`) and sent no one-of params (4 spurious
   `-32602`). All 7 "failures" were mine. The finding only appeared in pass 2, when the probe sent what the
   **MCP's own schema** declares — because that schema, not my guess, is what a model actually sends. The
   ledger's lesson 1 held on its first re-test this session.
2. **Two of my three suspicions were wrong, and reading the code killed them.** `watchpoint_add` looked
   like it might silently ignore the MCP's `read`/`write` booleans; it honours them (`op: "read"` verified
   on the wire) and refuses `read:false, write:false` rather than defaulting. `run_to` looked like it
   reported a wrong symbol; it returns `reached: false` with an explicit caveat that *nothing about the
   machine state follows from where it stopped*. Both were misreads of my own probe output.
3. **The bugs were both in the client, and both were "true by accident against one implementation."** That
   is precisely the value D10 predicted a second implementation would provide, and it is now measured
   rather than argued.
