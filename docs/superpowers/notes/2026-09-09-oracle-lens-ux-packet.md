# Oracle Roster C (UX seat pair) — packet

**Late panel on the 09-06 corpus. Ran at its own NEW pin `cb21f43`; the 09-06 packet is amended to
name this pair as late plus this pin, and is NOT re-dated.**

- **Seats:** UXa (task walk, five newcomer jobs, README only) · UXb (heuristic audit, every panel,
  control and message against the fixed checklist). ×2 opposed; they did not read each other.
- **Charter:** `2026-09-07-oracle-lens-ux-CHARTER.md` (amended four times during this run — see
  *What the brief got wrong*). **Rig:** `docs/2026-09-09-uxrig-isolation-proof.md`.
- **Surfaces:** both windows — `oracle-frontend` (minifb, the game window) and `oracle-player`
  (egui + `egui_dock`, the debug tabs). Every finding names its window.
- **Out of scope, and therefore UNEXAMINED rather than cleared:** pacing/smoothness/responsiveness
  (a virtual display has no vsync), audio by ear, gamepad, the owner's real display.
- **Isolation:** both seats reported **PROVEN**, two-sided, from kernel state — see the proof doc.
  The owner's live window (pid 1570308) was never signalled and nothing reached his display.

---

## ⚑ CONVERGENCE — independent seats, opposed walks, same target

The panel's strongest signal. Five, and the first two are the packet's headline.

| # | target | UXa met it as | UXb met it as |
|---|---|---|---|
| C-1 | **Memory panel greets you with a refusal** | job 4: first click, `0xFFFF0000` already REFUSED | F-UXB-4: pre-filled value its own range check rejects |
| C-2 | **A refusal clipped mid-sentence, actionable half lost** | finding 8: Breakpoints refusal cut at "…so the table is" | F-UXB-5: right-dock clips both axes, no scrollbar |
| C-3 | **The two windows are two machines and nothing says so** | finding 2: halted one while the other kept drawing | disagreement 5: the charter *presumes* one machine |
| C-4 | **The game window answers to stdout, not to the person** | finding 12: job 2's full answer printed to stdout | F-UXB-21/22/24: ~30 commands announced only there |
| C-5 | **`--pid` is a vacuous filter for the game window** | isolation: used `--wm-class`, the discriminating one | disagreement 7: ran the positive control first |

C-3 is the strongest of the five **because the seats reached it from opposite ends**: one by
experiencing the divergence, one by noticing the charter's own wording assumes it cannot happen.

---

## Findings, most severe first

Verification column: **✔ firsthand** = re-derived by the controller against the pinned tree;
**seat** = carried on the seat's evidence, which is committed.

### 1 · `oracle-player` — the delete button is undrawable, and lands beside a real tick-box · **✔ firsthand, root cause found here**

UXb (F-UXB-8, its top finding) lost a breakpoint to a control **before it knew the control
existed**, while trying to re-enable one. Each row carries a real tick-box and, 36 px to its right,
a button whose whole label renders as an **empty square** — visually a second, unticked tick-box.
It deletes. No tooltip, no confirmation, no undo. Reproduced by coordinate: `x=892` →
`enabled:false`, row stays; `x=928` → `ok: {"removed":1}`, row gone. **The panel's own instruction
points straight at it**: *"or untick them one at a time in the Breakpoints tab."*

⚑ **The mechanism is not a design choice, and neither seat could reach it** (UXb's brief says press,
do not read). **The label in source is `✕` (U+2715)** — `ui.rs:2198` and `ui.rs:2351` — a perfectly
sensible delete icon. **The window cannot draw it.** `oracle-player` installs no `FontDefinitions`,
so egui's four bundled faces are in force, and U+2715 is **in none of them**:

| glyph | codepoint | covered by |
|---|---|---|
| `▶` | U+25B6 | Hack, emoji-icon-font, NotoEmoji |
| `◀` | U+25C0 | Hack, NotoEmoji |
| `⏸` | U+23F8 | emoji-icon-font |
| `∞` `·` `…` | — | Hack, Ubuntu-Light |
| **`✕`** | **U+2715** | ***none*** |

Every other symbol the UI uses is covered, which is the positive control proving the test can see
coverage. So the glyph falls back to the replacement box — **and the replacement box for a delete
button, rendered next to a tick-box, reads as an unticked tick-box.**
⚑ **The crate already knows this failure mode exists**: `screen.rs:124-164` carries a long analysis
of `◻` as the replacement glyph and how hard it is to detect. It reasons about the hazard in one
module while shipping a button that triggers it in another.
**Fix is small and has two independent halves** — a text label (`remove`) needs no font at all; a
bundled face that covers the glyph fixes every future icon. Prefer the label: it also answers
F-UXB-13 below, which no font can.

### 2 · `oracle-player` — the same square means *disable* in one tab and *delete forever* in the next · seat (UXb F-UXB-13)

In Watchpoints the square glyph is the **first** control in the row and there is no tick-box at all;
in Breakpoints it is the **second**, after a real tick-box. **Leading square = disable here, destroy
there.** This survives any font fix and is the reason the recommendation above is a word, not an icon.

### 3 · Both windows — two independent machines, and nothing on screen says so · **✔ firsthand (settled from source, as UXb asked)** · C-3

08:03:50Z: the debug window's top bar read `HALTED BY BREAKPOINT b0 at 0x00002334
(VBlank_Handler)` while the game window's title advanced `draws 45579 → 45779` in the same seconds.
Same ROM path, same art, same game.

**Settled:** they are two processes, each owning its own `System`, each binding its own socket. A
client attaches to one. **There is no mode in which they share a machine**, so the divergence is
correct behaviour and the defect is entirely that neither window says which machine it is.
Launching both is the *only* way to have the game and the debug tabs at once, so a newcomer who does
the obvious thing believes the tabs debug the game. **Job 3 succeeds in one window with no effect on
the game being watched in the other.**

⚑ **Protocol delta (UXa's, and it belongs upstream).** Roster C's fifth rule governs resolvers
falling through to a shared last resort. **This is the same family with nothing falling through**:
both windows bound their own private socket, both resolved exactly as instructed, both were correct
— and a person still cannot tell which of two live machines they are looking at. **A resolver that
never falls through can still leave the user attached to the wrong one of two correct answers.**
Aurora's axis was *reaching*, ours *becoming*, sigil's a third; this is a fourth and it is not a
direction at all — it is whether the resolved identity is **displayed**. It surfaced as a UX defect
only because both machines belonged to the same seat.

### 4 · `oracle-player`, Memory — the panel opens already refusing its own default · **✔ firsthand** · C-1

First click, nothing typed: address `0xFFFF0000`, and beneath it
`REFUSED -32004: only cartridge ROM ($000000..rom_len) and work RAM ($E00000-$FFFFFF) are readable
in this slice`. Reproduced on a fresh instance. `memory.rs:694` sets that default; `ui.rs:1589`'s
hint text **advertises the same refused value**.

⚑ **We documented this hazard and then shipped it as the default.**
`tests/contract/bus-protocol.schema.json:2110` explains at length that a 68000 listing writes work
RAM as `0xFFFF8CFA` while the bus is 24 bits wide, that `addr` is the field to compute with, and
that passing the listing spelling is *"refused loudly … so a joining client that takes the wrong
field pays a round trip, never gets a wrong byte"* — signed *"(oracle, 2026-08-29, found by hitting
it)"*. **The panel's default is that spelling.**
**And the same field has two behaviours**: typing the symbol `VBlank_Flag` returns
`{"addr":"0x00FF8000", "rawAddr":"0xFFFF8000"}` and reads it — the field does the masking silently
for a symbol and refuses it for a number, explaining neither.

### 5 · `oracle-frontend` — the game window silently drops typed characters · seat (UXa 3)

`break` → `RAK`; `abcdefgh` → `ABCDFGH`. **Control**, same driver, same display, same loop, into
`oracle-player`: 8/8. The window is the variable, not the rig. It cost the seat its palette survey.
⚠ **Caveat kept, the seat's own:** minifb samples keys per frame under llvmpipe, so **the drop RATE
is not transferable** — the control establishes existence, not rate. This is input correctness, not
pacing, and is therefore in scope.

### 6 · `oracle-frontend` — a no-match search says nothing at all · seat (UXb F-UXB-23)

Type a term matching nothing: no message, and Enter does nothing; pixel-identical before and after.
**The same build's other palette answers the identical mistake** with *"…62 are, and the list above
is all of them. Nothing was sent."* Two palettes, opposite behaviour, and **the silent one is a
newcomer's first contact.**

### 7 · README — the front door does not know the debug window exists · **✔ firsthand** · (UXa 4)

`grep -c oracle-player README.md` → **0**. Layout says *"Four crates"*; `crates/` holds **six**. The
omitted crate is the entire Registers/Memory/Objects/Screen/Breakpoints surface — **the window that
does four of the five day-one jobs**. The README also never says where a ROM comes from: both run
recipes are `<rom.bin>`, and the only ROM named is "Aeon's debug ROM" with no path and no build step.

### 8 · `oracle-frontend` — three of the five day-one jobs cannot be done there at all · seat (UXa 5)

Full palette walked end to end: no breakpoint entry, no memory viewer, no live-object list (SPAWN
OBJECTS places, it does not list). **Compounding 7:** the front door sends a newcomer to the window
that cannot do the jobs and never mentions the one that can.

### 9 · `oracle-player`, Breakpoints — refuses an inexact symbol and will not help, while the same process serves prefix search one button away · seat (UXa 6)

`VBlank` → `REFUSED -32013: no symbol named VBlank`. 3102 symbols loaded, none browsable, no
completion. **2m34s** to recover, by hand-writing `emulator/lookup_symbol {"name":"V"}` in the
`commands` console — **whose own doc string is *"bounded prefix search when not an exact match"***.
It returned `VBlank_Handler`, which armed and hit first try.

### 10 · `oracle-player`, Screen — an enabled control that cannot work, next to a tab that disables its own · seat (UXb F-UXB-17)

Row says `slot 0 (empty)`; the enabled `load` answers `No such file or directory (os error 2)` — a
C errno surfaced to a person. **One tab away, Effects disables the impossible control and explains
on hover.** Same window, same build, opposite conventions.

### 11 · `oracle-player`, Objects — two adjacent numbers answer "what is alive" differently · seat (UXa 7)

Header: `object pool 11 active of 66 slots`. Four rows below, an **untitled** card whose first row is
`live 7`. The card is about rings — its closing sentence says so — **but the disambiguating sentence
arrives after the number.**

### 12 · Both windows disagree on `--help` · **✔ firsthand** · (UXa 9)

`oracle-player`: `-h | --help => usage()` (`main.rs:305`), exit 64. `oracle-frontend`: **no `--help`
arm at all** — it falls to `unknown flag` (`main.rs:406`, exit 2), and `-h` does not start with `--`
so it is taken as a **positional**, i.e. a ROM filename: `cannot read ROM -h: No such file or
directory`. Never mentions the flag, never prints usage.

### 13 · `oracle-player` — a long status line evicts the toolbar's own controls · seat (UXb F-UXB-1)

Pressing `step` writes a status line long enough to push `commands` and `open ROM` off the window.
Not transient.

### 14 · `oracle-player` — `poke` has no undo and never reports what it overwrote · seat (UXb F-UXB-7)

The seat's own `poke DEAD` crashed the game into its ADDRESS ERROR screen. **Declared as a confound:
no finding rests on it.** It is the lived proof of the defect, and the only way back was
`emulator/reset` from the palette — **the player window has no reset control of its own.**

### 15 · `oracle-player` — volume and mute report success for an audio device the process knows is absent · seat (UXb F-UXB-25)

Filed with the seat's own tension declared: the charter bans audio *findings*, and this needs no ear
— only the window's text against the process's own stdout. **Ruled: in scope.** The bar exists to
stop a seat judging sound it cannot hear; a control that reports success for a device its own
process has already failed to open is a truthfulness defect, not an audio one.

### 16 · `oracle-frontend` — the register lens clips on the LEFT and shows no numeric PC · seat (UXa 11)

Renders `ORLDLINES.NO_SWEEP+$4`; `oracle-player` prints `PC 00002334` for the same moment.

### Remaining
UXb F-UXB-2/3/6/9/10/11/12/14/15/16/18/19/20/26 and UXa's palette-scroll and truncation items are in
the seats' own files. **Six CLEAN walks** with step counts and per-step captures (Breakpoints arm,
Registers, Objects, commands palette, Screen state/layers, Planes, Effects, open ROM, frontend watch
keys) — a clean verdict that is examinable rather than "nothing found".

### Look / taste — captures for the owner, NOT packet findings
28 across the two seats: register file needs a scroll, several panels clip horizontally, the palette
hotkey column collides with labels, the Memory write hint tells a GUI user to *"call `emulator/pause`
first"* when a pause button sits in the same top bar.

### Standing findings, met as a person
`STYLE-NUMBER-BAKEOFF` — **met here as** a panel that opens by asking a newcomer to choose between
two typographic treatments. Booked, not re-found.

---

## What could not be driven, and what would

| not driven | why | instrument that would |
|---|---|---|
| audio | `snd_pcm_open: Host is down` under the private `XDG_RUNTIME_DIR` — by design | the owner's session |
| gamepad | needs `/dev/uinput`, forbidden: unbound to any display, lands wherever focus is | a display-bound injector, or his session |
| pacing / vsync | out of scope; the player itself says `Pacing is UNMEASURED` | his real display, freshly authorised |
| `Ctrl+O`, dock splitters | XTEST rig has no modifier chords and no drag | a driver with chord + drag support |
| the owner's stored layout | never read or written, deliberately | — |

⚑ **The splitter gap has a consequence worth stating: it is the only user remedy for the clipping in
finding 2/C-2, so that finding may read worse than a person with a mouse would experience it.** UXb
named this rather than letting the severity stand unqualified.

---

## What the brief and the charter got wrong

**The charter was amended four times during this run, every time because someone disagreed with it.**
On a pilot the brief is as much under test as the window.

1. **The absent-half of the safety proof was pointed at the wrong surface** — `:0` only, on a Wayland
   desktop, which returns clean during the exact escape it exists to catch.
2. **`/dev/uinput` was banked as root-only and is ACL-open** — a mode string is not a permission.
3. **The absence filter was vacuous for one window** — minifb sets no `_NET_WM_PID`. *An absence
   check is only evidence once the same filter has been shown to FIND the thing somewhere.*
4. **Existence stood in for completeness** on the ROM guard, and **the handover's own wait-loop would
   have snapshotted a truncated image** — `-s` fires on the first byte of an in-place rewrite. It also
   blocked, idling a seat through an outage most of its walk did not need a ROM for.
5. **`--help` output was unnamed.** Help text is a product surface, not source. UXa treated it as in
   bounds and got three findings in twenty seconds; a literal seat either skips the cheapest surface
   in the product or books reading it as a violation. **Name it.**
6. **The window table reads as a panel inventory and is not one.** It lists six; `oracle-player` also
   has Planes, Spawn, Effects, Breakpoints, Watchpoints, Profiler and a `commands` console — and that
   console was the only way out of the breakpoint dead end. *A list that reads as complete*, which is
   the shape the charter itself warns about.
7. **"Failure to get a ROM in is FINDING NUMBER ONE" conflated two things.** The ROM went in fine;
   what failed was the README's ability to say where a ROM comes from. The framing invites a seat to
   write a **rig condition** up as a **product finding**.
8. **The F2 ban contradicted the snapshot rule in the same document.** States are written beside the
   ROM, so after the mandated snapshot they land in the seat's own tree. Obeying the ban literally
   would have deleted six controls and one finding. **Correct rule: read the state path off the launch
   line and press F2 only if it is inside your own tree.** UXb proved aeon's `s4.debug.state0`
   byte-identical before and after.
9. **"Look and taste are not findings" ships without a test.** UXb supplied its own — *does the person
   lose information they need to act?* — and flagged that the next seat will invent a different one.
10. **"No audio findings" bans a subject where it means to ban a kind of evidence.** See finding 15.
11. **The two documents pin different SHAs** (brief `cb21f43`, handover `e06a371`); `tools/uxrig/` is
    identical at both, so nothing was affected, but it is exactly the ambiguity the pin exists to remove.

**Owed to the hub:** this packet's landing SHA, and the paragraph above. Items 1-4 were sent early, at
the hub's request, and are already banked and relayed to sigil and aurora.

## ⚑ ADDENDUM 2026-09-09, for whoever fixes C-3: DO NOT SHORTEN THE SOCKET PATH TO ITS BASENAME

Landed from aurora's own bus-identity parcel, relayed via the hub and **verified against our tree
before booking**. Their remedy for the identical defect had to keep **two** path segments, derived
rather than chosen, because a basename label renders two different machines identically.

**It transfers to us exactly, and worse, because the collision is in our own resolver.**
`crates/oracle-aether/src/server.rs:66-78` resolves `$ORACLE_SOCKET` → `$EXODUS_SOCKET` →
`$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`. **The last two share the basename
`oracle.sock`**, so a window labelled with the basename would show our two machines as the same
machine — the C-3 defect reintroduced inside C-3's own fix, and **invisible to any test that only
asserts a label is displayed.**

Two further notes from that parcel, both worth inheriting:
- **Check how the value reaches the SCREEN, not whether it exists.** Their path was already correct
  in the payload and already returned correctly by the connect handler; the push that every later
  status change travelled on passed no argument, so the badge would have shown the path at connect
  and lost it at the first pause. Nobody predicted it; it surfaced only by tracing the update path.
  Same family as our own `Host::pump` snapshot defect: **correct at one door, lost at another.**
- **A warning as loud as a hover is half-surfaced** — their `identityWarning` had been computed for
  months and displayed nowhere.

Measured here: **neither of our windows displays a socket path today**, so nothing is currently
wrong. This is a forward hazard on the C-3 fix, not an open defect.
