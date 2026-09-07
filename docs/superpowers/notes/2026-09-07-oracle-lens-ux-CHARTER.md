# Oracle Roster C (UX seat pair) — CHARTER and seat rules

**Pinned review SHA: ⟨SET AT LAUNCH⟩** — a NEW pin taken when the in-flight lens-fix parcel lands, not
the 09-06 packet's `d3ca871`. Roster C is a **late panel** on a corpus that already has a packet: it runs
at its own pin, and `2026-09-06-oracle-lens-sweep.md` is amended naming this pair as late plus this pin.
**The packet is never re-dated.**

Authority: owner, ruled 2026-09-07, relayed by empyrean-c0 and verified firsthand here — empyrean
**`3ad431f`** is an ancestor of their `origin/main`, `--stat` shows a docs commit carrying a docs ruling,
and his words are in the blob: *"I think it should have one right?"* and *"I think we draft it and run on
oracle, aurora, and sigil for now"*. **This lane is the pilot; sigil then aurora run it next.**

⚠ **THE REVISION THE PACKET CITES IS aeon `8def2240`** (`docs/superpowers/LENS_PROTOCOL.md`), not the
empyrean amendment: Roster C is now IN the protocol, and the protocol is aeon's file. The empyrean chain
is the drafting history — `97cd725` ruling text, `6a12740` this lane's four pre-run gaps, `3ad431f` the
fifth rule — and piloting from any of them re-inherits whatever was added after it.

**Verified firsthand, and the verification is worth more than the result.** aeon `8def2240` is an ancestor
of their `origin/master`, `--stat` shows one docs file +59, and all five seat rules plus all four of this
lane's gaps are present (matched on wording unique to each: *"more than one surface"*, *"FINDING, never
BLOCKED"*, *"OUT OF SCOPE for Roster C"*, *"binds the CLIENT"*, *"shared last resort"* — 1 hit each).
⚑ **AND THE FIRST PASS OF THAT CHECK REPORTED THREE OF THE FOUR AS MISSING, WRONGLY.** The pattern was
`grep -ciE "vsync\|pacing\|responsiv"` — BRE escaping inside an ERE, where `\|` matches a *literal pipe*
rather than alternating. It returned 0 against a file that says "Pacing, smoothness and responsiveness"
in terms. **The world was fine; the question was malformed**, and an empty result reads exactly like a
clean finding. This is the protocol's own alphabet defect, committed here while checking a peer's claim,
and caught only by reading the landed section instead of trusting the count. The first loop in that check
carried a control and the second did not — **the loop without the control is the one that lied.**

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

## Seat rules (transcribed from `3ad431f`, not summarised)

- **Own instance, private display, private socket.** Xvfb, **X11 forced**, screen size **verified from
  inside the display**. Never the shared server, never the owner's display, **never the emulator MCP**.
- ⚑ **"Private socket" binds the CLIENT, not only the instance** (gap 4). Our server takes `--socket` and
  `$ORACLE_SOCKET` cleanly (`crates/oracle-aether/src/server.rs:66-78`); the suite's **reference client**
  resolves on a directory test and can reach the shared path whatever the server was told. **The seat
  points its client explicitly and says which client it used.**

## ⚑ THE RIG IS MECHANISED, NOT ASSERTED (aurora's two hazards, and one of them is safety-critical here)

Relayed by empyrean-c0 from aurora's own UX charter (aurora `origin/review/aurora-lens-sweep` `4abfc723`,
`docs/reviews/2026-09-07-lens-ux-charter.md`). **These are aurora's findings, from building this rig
before us; they are not re-derived here and are attributed rather than adopted as our own measurement.**

1. ⚑ **A raw launch attached to the OWNER'S COMPOSITOR instead of the Xvfb — in their own red-first
   proof.** For us that is not an untidy run, it is the one outcome this lane's flat rule forbids: a
   window on his screen while his player may be live. **So forcing the display is the LAUNCH HELPER'S
   job, never a flag a seat is trusted to pass.** The helper sets and forces the private `DISPLAY` for
   both `oracle-frontend` and `oracle-player`, and **the seat proves once, with a capture, that a launch
   with the helper lands on the private display and that a launch without it REFUSES rather than falling
   back.** Proven once, at the start, not asserted per launch. A rig that merely *usually* uses the right
   display is the same artifact as one that always does, right up until it is not.
2. **Naming a BINARY silently measured the main checkout's build** when aurora's per-seat worktree was
   not runnable. **The built TREE is named, with no default.** If a seat drives a per-seat build out of a
   worktree, it names that worktree; if it cannot build there, that is a BLOCKED report, never a quiet
   fallback to whatever binary was on the path. This lane already has the matching scar from the other
   direction — *a merged serve is not a served method, the consumer reaches a BINARY* — and a stale
   binary answers with total confidence.

⚑ **Both are the same shape and it is the shape this whole panel exists to catch: a default that fills in
silently when the specific thing is absent.** A display that falls back to the compositor and a binary
that falls back to the main checkout are one defect wearing two costumes.

**That sentence is now the fifth Roster C rule** (empyrean `3ad431f`, verified here as a reachable
ancestor and a docs commit carrying docs): *no resolver in the rig may fall through to a shared last
resort.* Aurora turned it on their own charter and found a third instance — their app-side socket
resolver reaching the owner's live game window when the variable was unset, reachable **because UXb is
required to press every control and the status badge is one.**

⚑ **THIRD INSTANCE, TURNED ON OURSELVES — AND OUR ARROW POINTS THE OTHER WAY. Measured, not
assumed.** Aurora's instance is a client that **connects** and can land on the owner's live server. Ours cannot take that
form: `oracle-player` and `oracle-frontend` **bind**, and `Server::bind` connects first and returns
`AddrInUse` — *"another Aether server is already live on {path}"* — against anything that answers
   (`crates/oracle-aether/src/server.rs:343-353`, read at `d9fb676`, not taken from the doc comment that
   claims it). **So a seat cannot reach his window through this door, and the guard is real code.**
   **The same class is still reachable in the MIRROR direction, and that is the one our rig must close:**
   a seat launching with bare `--aether` and no `--socket` binds the **shared well-known path**
   (`$ORACLE_SOCKET` → `$EXODUS_SOCKET` → `$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`). If his
   window happens to be **down** at that moment there is nothing to refuse, the seat's throwaway instance
   takes the shared socket, and the next lane's client resolving that same chain attaches to a scratch
   machine while believing it reached his. **So: `--socket <seat-private path>` is MANDATORY and bare
   `--aether` is forbidden in this rig**, and the seat proves once that it bound the private path.
   ⚑ **The durable half for sigil, who inherits the rule and not our instances: the fifth rule's wording
   is right and the hazard's DIRECTION is per-tool.** Aurora's risk was reaching the shared thing;
   ours is *becoming* it. A rig audited only for aurora's arrow would have passed ours.

⚑ **AMENDED BY SIGIL'S CORRECTION, AND IT LANDED ON THIS SEAT'S OWN REASONING.** Sigil's form: *a tool can
be exposed in BOTH directions at once, and the becoming one is the one that looks already handled.* The
paragraph above checked one arrow and then declared the other, which is the shape it was warning about.
**Re-measured here, and the answer is better than the first one:**

* **Reaching, via our windows: DOES NOT EXIST.** Neither window has a production client. Every
  `UnixStream::connect` in `oracle-player` and `oracle-frontend` outside `oracle-aether`'s own bind probe
  is inside a `#[cfg(test)]` module — `drain.rs`'s `Peer` sits under `mod tests` at `:248`, and a non-test
  build of the crate names it nowhere. Checked a second way rather than by eye: the symbol does not
  survive a non-test build.
* **Reaching, via the SEAT'S OWN TOOLING: REAL, and already closed** by the client-binding rule above
  (gap 4). This is the half the first pass got right for the wrong reason: it is exposure in aurora's
  direction, living in the rig rather than in the product.
* **Becoming: REAL**, closed by mandating `--socket <seat-private path>`.

**So we are exposed in both directions after all, and the distinction that matters is not which arrow but
WHICH ARTIFACT CARRIES IT** — the product only becomes, the rig only reaches. An audit that asks "is this
tool exposed?" gets one answer; asking it separately of the product and of the rig gets two.

**Sigil's third arrow, and why it corroborates rather than echoes:** their becoming-exposure revealed that
three rules they already ran *separately, each from its own incident*, are one class — *this tool's run
becomes what a later run resolves to*. Their charter's own instance is the sharpest of the set and is a
warning we should read for ourselves: **their nightly scripts append to a state ledger carrying revision
and time but no WRITER**, so a seat running them corrupts the trend invisibly. Their seat now gets its own
state dir. ⚑ **Checked here: this repo's shared appendables are `docs/lane-log.jsonl` and
`docs/decisions.jsonl`, both already barred to seats by the standing no-agent-edits rule, and `target/land/`
run dirs, which are timestamped per run and not a trend. No unwritered ledger found — but the check is
recorded because "we have none" and "we did not look" are the same artifact.**

**RULE FIVE'S PROOF OBLIGATION, AS GENERALISED BY THE HUB** (on aurora's measurement that their chains
cannot refuse without app changes a seat must not make): *terminate the chain explicitly and prove which
value was in effect, read back from the run's own output; **a demonstrated refusal is one such proof, not
the only one.*** **Our launch-helper refusal proofs stay valid exactly as written above** — the widening
adds an option where refusal is unavailable, it does not retire the stronger form where it is.

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
2. **One paragraph on what this brief STILL got wrong**, and it now carries a second passenger by the
   hub's own sequencing: **the reaching-versus-becoming direction point rides IN that paragraph as one
   protocol delta**, rather than going up as its own edit. The hub's reason is this document's own bar 18
   — the protocol text landed minutes ago at aeon `8def2240`, and moving it per finding is relay spam.
   Sigil's instance turned out to be a **third arrow** (a gate test resolving into `oracle-old` when a
   variable is unset, with the tree present so nothing refuses), which is the evidence the generalisation
   was worth making: three tools, three directions, one rule. **Do not let the delta be dropped because
   the paragraph is what was asked for.** — sigil runs the pair next, and on this first run
   **the brief is the thing under test**, not the window.
