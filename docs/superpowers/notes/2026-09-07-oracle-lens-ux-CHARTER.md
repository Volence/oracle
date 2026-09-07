# Oracle Roster C (UX seat pair) — CHARTER and seat rules

**Pinned review SHA: ⟨SET AT LAUNCH⟩** — a NEW pin taken when the in-flight lens-fix parcel lands, not
the 09-06 packet's `d3ca871`. Roster C is a **late panel** on a corpus that already has a packet: it runs
at its own pin, and `2026-09-06-oracle-lens-sweep.md` is amended naming this pair as late plus this pin.
**The packet is never re-dated.**

Authority: owner, ruled 2026-09-07, relayed by empyrean-c0 and verified firsthand here — empyrean
**`6a12740`** is an ancestor of their `origin/main`, `--stat` shows a docs commit carrying a docs ruling,
and his words are in the blob: *"I think it should have one right?"* and *"I think we draft it and run on
oracle, aurora, and sigil for now"*. **This lane is the pilot; sigil then aurora run it next.**

⚠ **RUN FROM `6a12740`, NOT `97cd725`.** The earlier revision is the ruling text; the later one carries
this lane's four pre-run gaps folded in. Piloting from the earlier one silently re-inherits all four.

## The two seats

**UXa — task walk.** Attempts the five jobs below, in order, with **no instructions on how**. May read
`README.md` **and nothing else**; the moment it opens source to make progress, that is a finding and the
job continues from there. Logs every stall, every guess, and every dead end, ranked by time burned.

**UXb — heuristic audit.** Walks every panel, control and message against the fixed checklist: **can it
be found · does it answer back · is it consistent with its neighbours · can a mistake be undone · does an
error say what to do next.** Findings name the panel or the message.

The pair is ×2 **opposed**, and convergence between them is the top finding class. They do not read each
other's output and they do not run beside any other seat.

## ⚑ THE SURFACE IS NAMED, because oracle is TWO windows

Folded into the amendment as gap (1) and it is this lane's own scar: a spawn-picker parcel was briefed
against artifacts in one window and ruled about the other. **Both are in scope and every finding names
which one it is in:**

| surface | crate | what a person meets |
|---|---|---|
| the game window | `oracle-frontend` (minifb) | the running game, its own key bindings, click-to-inspect |
| the debug tabs | `oracle-player` (egui + `egui_dock`) | Registers, Memory, Objects, Screen, Pacing, nav |

A finding that does not name its window is not accepted. Where the two disagree about the same fact,
**that disagreement is itself the finding** and outranks either half.

## The five jobs (UXa), named here so the seat cannot choose easy ones

Each is a thing a newcomer would actually want done on day one. None names a method, a panel, or a key.

1. **Get a game running and see it.** From a clean checkout and the README alone, reach a ROM rendering
   in a window. ⚑ **If the README does not carry a newcomer from launch to a loaded ROM, that is finding
   number one and the most valuable thing this seat can return — it is NOT a reason to stop.** Report it
   and proceed by whatever means, logging what was needed.
2. **Find out why something on screen looks wrong.** Pick any visible pixel and establish what drew it —
   which layer, which tile, which palette.
3. **Stop the game at a chosen moment.** Set a breakpoint on a named thing and see it actually hit.
4. **Read a value at that moment.** With the machine stopped, find the contents of a CPU register and of
   a memory address.
5. **See what the game currently has alive.** Find the live objects/sprites and identify one of them.

## Seat rules (transcribed from `6a12740`, not summarised)

- **Own instance, private display, private socket.** Xvfb, **X11 forced**, screen size **verified from
  inside the display**. Never the shared server, never the owner's display, **never the emulator MCP**.
- ⚑ **"Private socket" binds the CLIENT, not only the instance** (gap 4). Our server takes `--socket` and
  `$ORACLE_SOCKET` cleanly (`crates/oracle-aether/src/server.rs:66-78`); the suite's **reference client**
  resolves on a directory test and can reach the shared path whatever the server was told. **The seat
  points its client explicitly and says which client it used.**
- **Every finding ships a screenshot, or the diagnostic text verbatim. No evidence, no finding.**
- **A clean task still ships its step count with a screenshot per step**, so a clean verdict is
  examinable rather than "nothing found". This is the house failure mode aimed at the sweep itself.
- **Look and taste are NOT findings** — colour, placement, wording preference are captures for the owner
  under the standing look/taste rule. Usability (lost, stalled, undone by the tool, misled by a message)
  goes in the packet's normal bins.
- ⚑ **Pacing, smoothness and responsiveness are OUT OF SCOPE** (gap 3). A virtual display has no vsync,
  so a "feels sluggish" reading taken there answers a different question while looking like an answer
  (`F-VSYNC-NEVER-MEASURED`). Those need the owner's real display and are his captures.
- Each seat closes with **what it could not drive** (gamepad, audio, a device it lacks) and what would.

## Standing findings — do NOT re-find these as new

The 09-06 packet is `2026-09-06-oracle-lens-sweep.md`: 8 CRITICAL, 33 HIGH, ~77 MEDIUM/LOW. Its
UI-adjacent open rows are already booked and are not UX findings: `F-PANELS-INVISIBLE-TO-SCREEN-TEXT`,
`F-PLANE-RASTER-UNGUARDED`, `F-AUDIT-PAGE-CONTRADICTS-ITSELF`, `F-PLANES-RASTER-EVERY-FRAME`, and the
`STYLE-NUMBER-BAKEOFF` / `DATA-DISPLAY-AUDIT` rows waiting on the owner's A/B answer. The older narrow
artifact is `docs/2026-08-17-player-s3-lenses.md` (player S3 only); its open follow-ups `F-PALETTE-SCROLL`
and `F-PALETTE-HINT` are likewise not new.

**A seat MAY report one of these as a UX finding if and only if it met it as a person**, and must say so:
"booked as X; met here as ⟨what the newcomer experienced⟩". A booked defect and a usability defect can be
the same line of code and are different findings.

## Owed to the hub after the run, both

1. **The landing SHA of the amended packet.**
2. **One paragraph on what this brief STILL got wrong** — sigil runs the pair next, and on this first run
   **the brief is the thing under test**, not the window.
