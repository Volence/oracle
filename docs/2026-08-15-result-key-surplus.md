# The complete result-key surplus (2026-08-15) — ruling condition 7

The CR-13/CR-14 ruling made this a **precondition** of writing the 12 missing schema fragments:

> *Before step 4 is written: rerun the key-set sweep across all advertised methods' success branches — my
> probe found four surplus sets in ten minutes that the CR's 33 messages missed; the amendment should
> enumerate the complete surplus once, not per-discovery.*

It was right to insist. This sweep drove **every one of the 21 advertised methods**, including every
distinct success *branch* (both `read_memory` addressing modes, all three `lookup_symbol` directions,
`state_hash` with and without the framebuffer, `checkpoint_list` truncated and not, `run_to` reached and
deadlined, `checkpoint` with and without a label, `checkpoint_drop` by id and `all`). Script:
`scratchpad/sweep.py`.

**It found four more methods that neither the 33-message probe nor the ruling reached.** The floor was
never 10 methods. It is **16**.

## The surplus, complete

Envelope fields (`frame`, `mclk`, `running`, `droppedEvents`) subtracted throughout. **Bold** = found by
this sweep and by neither of the two passes before it.

| method | §6/§4/§2.1 row | surplus on the wire |
|---|---|---|
| `initialize` | §2.1's six keys | `limits`, `methodSummaries` |
| `emulator/status` | `running,pc,sp,sr,symbolAtPc?,frameToken,symbolCount,romLoading?` | `romBytes`, `romPath`, `symbolsPath`, **`symbolDisp`** |
| `emulator/registers` | `d0–d7,a0–a7,pc,sp,sr` | `usp`, `ssp` |
| ~~`emulator/run_to`~~ | `target,reached,pc,maxFrames,symbol?,symbolDisp?,caveat?` | ~~`stoppedAtFrame`, `stoppedAtMclk`~~ — **ruled out and REMOVED**, `f36b548` |
| `emulator/pause` / `resume` | *(no result)* | `wasRunning` |
| `emulator/checkpoint_list` | `checkpoints[]{…},cursor?,truncated` | `total`, `returned`, `limit` |
| `emulator/read_memory` | `addr,len,bytes,symbol?` | `caveat`, `region`, `symbolDisp` |
| `emulator/read_vram` | `addr,len,bytes` | `caveat` |
| `emulator/state_hash` | `vram,cram,vsram,regs,combined,framebuffer?` | `caveat`, **`framebufferSource`** |
| `emulator/screenshot` | `path` | **`bytes`, `format`, `height`, `source`, `width`** — five keys against a one-key row |
| `emulator/press` | `buttons,frames,frameToken` | `port` (and `port` as an undocumented *param*) |
| `emulator/hold` | `buttons,down` | `port`, `held` |
| ~~`emulator/release_all`~~ | *(no result)* | ~~`released`~~ — a hardcoded `true`; **ruled out and REMOVED**, `f36b548` |
| `emulator/lookup_symbol` (exact) | §4: `addr,name,otherMatches?` | `ambiguous`, `demangled`, `rawAddr` |
| `emulator/lookup_symbol` (prefix) | *(as above)* | `caveat`, `exact`, `query`, and `otherMatches` in the wrong container (CR-14) |
| `emulator/lookup_symbol` (addr→label) | §4: `name,addr,disp` | `ambiguous`, `query`, `rawName`, `synthetic` |
| `emulator/load_symbols` | `path,symbolCount` | `binding`, `caveat`, `moduleCount` |
| `emulator/reload_rom` | `reloaded`\|`queued`, `path`, `diagnostic?` | **`romBytes`, `symbolsDropped`** |

## Five methods are clean, and the reason matters

`run_frames`, `checkpoint`, `restore`, `checkpoint_drop`, `pixel_attribution`.

Three of those five are the checkpoint methods, which is not luck: they were **specified before they were
implemented** (CR-2 was raised, ruled, and only then built), and §6.1 explicitly told the implementer that
`checkpoint`'s `frame`/`mclk` and the whole of `restore`'s result *are* the machine stamp and that "no
extra fields are needed and none should be invented." The implementation obeyed — `checkpoint` emits only
`id` and `bytes`, `restore` emits `{}`.

`pixel_attribution` is the fifth and it is the same story one day old: contract row first, handler second,
with the ruling's condition 4 requiring exactly the schematized keys.

**So the surplus is not carelessness — it is what happens when a method is built before its row is
written.** Every clean method was specified first. That is a stronger argument for the contract-leads
sequencing than any of the prose about it, and it is worth more than the individual key rulings.

## Consequences for the amendment

1. **`screenshot` and `reload_rom` must be ruled, and were not covered by the CR-13 ruling.** `screenshot`
   returning `width`/`height`/`format`/`bytes`/`source` is a materially richer contract than "returns a
   path" — `source` in particular reports *which* frame was written (the same raster-vs-stateRender
   distinction the hosted screen path already advertises), which is the kind of provenance D11 exists to
   make explicit. Likely register; not this pass's call to make silently.
2. **`state_hash.framebufferSource`** is the same family and should be ruled with it.
3. **`status.symbolDisp`** rides with `symbolAtPc`, which *is* catalogued — almost certainly a register.
4. **`lookup_symbol` carries eleven undocumented keys across its three branches**, which makes it the
   single largest unruled surface on the bus and confirms the ruling's instruction to settle §4 wholesale
   rather than inherit it.
5. The `caveat` count is now **five** methods (`read_memory`, `read_vram`, `state_hash`, `load_symbols`,
   `lookup_symbol`) plus `run_to`'s catalogued one — reinforcing the ruling's call to define it once in
   §2.4 rather than per row.
