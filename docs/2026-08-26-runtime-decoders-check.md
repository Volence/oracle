# RUNTIME-DECODERS — the four live checks, taken in the foreground

Closes the `⟨RUNTIME⟩` debt carried unchanged through `docs/2026-08-26-cr-d-object-decoders.md` §12.7,
`docs/2026-08-26-ruling-cr-d.md` §383 and `docs/2026-08-26-cr-d-amendment-handoff.md` §11. The CR tagged
four items, the adjudicator endorsed all four verbatim and attempted none, and applying the ruling added
none. **All four are now measured.** Nothing here was taken from source; every line below is a reply from a
running server or a count over transcripts.

Taken by the overseer in the foreground, as the CR requires — background agents must not touch the
emulator MCP, and none did.

## 0. What answers a consumer today — and the correction that matters

**The MCP shim spawns our own `oracle-aether`; it does not reach the legacy C++ server.**
`oracle-old/linux-port/mcp/oracle_mcp.py` **SPAWNs by default** — lazily, on first tool use, a private
server on a private `tempfile.mkdtemp(prefix="oracle-mcp-")` socket, from
`ORACLE_AETHER_BIN` (default `oracle/target/release/oracle-aether`) on `ORACLE_ROM`
(default `aeon/s4.debug.bin`, `:103`). It **ATTACHes only when `$ORACLE_SOCKET`/`$EXODUS_SOCKET` is
explicitly set** (`:131-137`).

⚠ **Correction, recorded because it was stated aloud before it was checked.** This pass first inferred
the shim ATTACHes to `/run/user/1000/oracle.sock` — i.e. to the owner's on-screen
`oracle-frontend --aether --x11` — by reading `resolve_socket_path()` in empyrean's reference client
(`clients/python/aether.py:36-48`). **That resolver is not the shim's policy.** The shim has its own, above.
Measured instead: this session's shim had no `$ORACLE_SOCKET`, and had **spawned its own child** on
`/tmp/oracle-mcp-9pbu07m4/oracle.sock`. **The owner's window was never touched**, and no call in this pass
mutated any state outside a private server this pass started and stopped. Same class as the
§"THERE IS NO SOCKET CHAIN" error in `OVERSEER.md`: a sound observation with the wrong resolver attached.

## 1. ⟨RUNTIME⟩ What the server actually replies — **ANSWERED (against ours; the legacy half is moot)**

Against a server built at `0f33c44` on the current `aeon/s4.debug.bin` + `s4.debug.lst`, which bound cleanly
(`symbols: 2708 symbols … (bound to this image)`). 44 methods advertised; `objectDecoders: true`;
`object_list`, `object_slot`, `player_state` all SERVED.

The CR asked two named sub-questions and both are now measured:

- **Is `class`/`name` ever non-empty in practice?** **Yes.** `{"slot":0,"name":"Player_Main","nameDisp":0,…}`
  — so `ObjCodeBase` resolved and the `code_addr → name` path works on a live build, not only in fixtures.
- **What does `player_2` look like in a one-player level?**
  `{"active": false, "addr": "0x00FF8E00", "role": "Player_2", "slot": 1}`.
  **This is ruling delta M5 — *role survives inactivity* — confirmed live.** M5 was decided on reasoning;
  it is now demonstrated doing exactly the job it was amended for.

Two more, unasked but load-bearing, because they are where the delta was spent:

- **An empty slot** (`slot: 65`) returns `{"active": false, "addr": "0x00FFA200", "slot": 65}` — **no invented
  numbers.** That is **M7**, the defect the implementer found while applying the ruling and held the handoff
  over, behaving as ruled.
- **Out of range** (`slot: 66`) is **refused, not clamped**:
  `[-32602] slot 66 is past the end of the object pool — this build has 66 slots (0..=65), and the bound is
  refused rather than clamped`.

**The legacy C++ server's own replies remain unmeasured.** Post-cutover that is no longer the question the
item was really asking: no consumer reaches it without setting `$ORACLE_SOCKET` by hand. Recorded as
deliberately not taken rather than done.

## 2. ⟨RUNTIME⟩ Do the pool symbols resolve? — **ANSWERED: ALL OF THEM**

All eight resolve in the current listing (`aeon/s4.debug.lst`, built 2026-08-26 11:24, 2708 symbols):

| symbol | address |
|---|---|
| `Object_RAM` | `FFFF8DB0` |
| `Player_1` | `FFFF8DB0` |
| `Player_2` | `FFFF8E00` |
| `Dynamic_Slots` | `FFFF8E50` |
| `System_Slots` | `FFFF9AD0` |
| `Effect_Slots` | `FFFF9D50` |
| `Object_RAM_End` | `FFFFA250` |
| `ObjCodeBase` | `10000` |

And the server builds `layout.pools` from them, live:

```json
"layout": {"baseAddr":"0x00FF8DB0","detectedBy":"symbol","detectedFrom":"Object_RAM",
           "engine":"aeon-sst","slotBytes":80,"slotCount":66,
           "pools":[{"name":"player","firstSlot":0,"slotCount":2},
                    {"name":"dynamic","firstSlot":2,"slotCount":40},
                    {"name":"system","firstSlot":42,"slotCount":8},
                    {"name":"effect","firstSlot":50,"slotCount":16}]}
```

**Consequence for the contract: §5.1's optionality on `pools` stays DEFENSIVE and does not become
load-bearing** — which is the outcome the CR flagged as the one to check.

`detectedFrom: "Object_RAM"` also confirms `decoders.rs:378`'s preference rule working on a real collision:
`Object_RAM` and `Player_1` share `FFFF8DB0`, and the mark is preferred over the label.

**Derived, not read off.** `slotBytes` was measured live as `Player_2 − Player_1` = `$50` = 80, which matches
the spec's `table_slot_bytes: 0x50` (so the mismatch guard at `decoders.rs:423` correctly did not fire), and
all three pool spans divide by 80 **exactly** — 3200/80 = 40, 640/80 = 8, 1280/80 = 16, total
`(A250 − 8DB0)/80` = 66.0. Three independent exact divisions is not what a wrong slot size produces. 66 is
also the slot count the legacy tool descriptions carry.

## 3. ⟨RUNTIME⟩ `Player_1` vs `$FF8DB0` vs the fixture — **ANSWERED, and it is decisive**

`Player_1 = FFFF8DB0`, and the server reports `baseAddr: 0x00FF8DB0`. **That is the demand doc's `$FF8DB0`,
exactly.** The demand is current and correct.

**The committed fixture is the stale one**, and by more than the CR knew. `aeon/tools/fixtures/s4_listing_excerpt.lst`
(last touched 2026-08-18, `a4ebf2d1`, for a budget parser) gives `Dynamic_Slots : FFFF8DC2` and
`Effect_Slots : FFFF9CC2`, against a live `FFFF8E50` / `FFFF9D50`. Its `dynamic → effect` span is
`$F00` = 3840 = 48 × 80 **with no `System_Slots` between them**; the live build splits that same region into
40 dynamic + 8 system + 16 effect. **The fixture predates the `System_Slots` split.**

**It could never have built `pools` anyway:** it is an *excerpt* and contains only 2 of the 8 symbols —
no `Object_RAM`, `Player_1`, `Player_2`, `System_Slots`, `Object_RAM_End` or `ObjCodeBase`.

So §2.4's *"one of them is from an older ROM"* resolves cleanly in favour of the demand, and the argument
it was making for D1 is now **measured rather than documentary**, as the CR asked.

## 4. ⟨RUNTIME⟩ Does anyone actually call these? — **ANSWERED WITH A COUNT**

§2.6's *form* assertion is now a number. Counted over 4,095 transcript files by parsing `tool_use` blocks:

| method | invocations | sessions |
|---|---|---|
| `emulator_player_state` | **154** | 20 |
| `emulator_object_list` | **39** | 10 |
| `emulator_object_slot` | **23** | 3 |

**216 invocations**, first on 2026-07-03, by repo: **aeon 193**, sigil 20, oracle/oracle-next 3.
The consumer who filed the demand is the consumer who calls it. This was not built for a hypothetical user.

⚠ **Method note — the obvious count is a confound, and it is a big one.** A naive
`grep -c emulator_object_list` over the same tree reports **10,265 mentions across 4,055 files**, and
reports ~4,055 files for *every* tool name including ones nobody has ever called. The MCP tool listing is in
every session's system prompt, so mentions measure *attachment*, not *use*. The near-constant across varied
tools is the tell — bar: *a clean constant across varied inputs suggests a confound.* Only `tool_use` blocks
count.

## 5. ⚑ The finding this pass was not looking for: **what we shipped was unreachable**

aeon's most recent real attempt, **2026-08-26T03:06:24**, called both methods and got, for each:

```
ERROR: [-32601] no such method: emulator/object_list
ERROR: [-32601] no such method: emulator/player_state
```

That call predates the serve (merged 12:05, logged 12:50), so it was legitimately unserved *then*. **But the
refusal did not stop when the serve landed.** `target/release/oracle-aether` was still the **25 Aug 21:03**
build — **never rebuilt after the merge** — and the shim spawns *that binary*. So every consumer session
spawning a shim between the merge and this pass got a pre-decoder server and the same `-32601`. This pass
reproduced it firsthand: this session's own shim, spawned 54 minutes before the checks, answered
`no such method` to both.

**Rebuilt at `0f33c44` (`cargo build --release -p oracle-aether`, exit 0), and the fix verified end to end
through the consumer's own path** — a fresh shim spawned exactly as aeon's session spawns one, no
`$ORACLE_SOCKET`, default ROM — which now returns the full `layout` block above at frame 0.

**Nothing further is needed from anyone: aeon gets the decoders on their next session.** No running server
was restarted, and the owner's on-screen player was not touched. It is still running its own pre-merge
build and will keep answering `-32601` until it is relaunched — worth doing when convenient, not urgent,
and **his call because it is his window**.

**The durable lesson, and it generalises past this parcel:** a merged, tested, pushed serve is not a served
method. The artifact a consumer actually reaches is a **binary**, and nothing in the merge rebuilds it. Same
shape as the rename fallout in `OVERSEER.md` item 1 — *compile-time-frozen paths, invisible until the binary
runs.* Here it was a compile-time-frozen **binary**, invisible until a consumer called.

## 6. Confirmed in passing

- **§12.8 stands, live.** `capabilities` really does advertise `romLoaded: true`, and the key is in neither
  artifact. Reported there, unacted-on there, unchanged here — it belongs to whoever next opens the handshake.
- **Ops:** a unix socket path under the session scratchpad exceeds `SUN_LEN` — the server refuses with
  `cannot bind the Aether socket: path must be shorter than SUN_LEN`. Use a short `/tmp` dir for probe sockets.

## 7. Not taken

- The legacy C++ server's own replies (§1), deliberately — moot for consumers post-cutover.
- No fix for the owner's running window; no state mutated outside a private server started and stopped here.
