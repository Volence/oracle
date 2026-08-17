# Player S1 shipped — command registry + palette spine (2026-08-17, overnight)

Branch `player-s1-palette`, 7 commits `6799c85..0fdbbbd`, merged to `m68000-microop-framework`.
Spec: `docs/superpowers/specs/2026-08-16-player-buildout-design.md` (§3–§4 are this slice).
Plan: `docs/superpowers/plans/2026-08-16-player-s1-palette-spine.md`.

## What shipped

- **`commands.rs`** — the command registry, single source of truth for every frontend action
  (~26 entries: visible + hidden aliases + number-key slot selects, audio rows `#[cfg]`-absent in
  no-audio builds). Registry invariants, subsequence matcher, key names, key→char, all tested.
- **`palette.rs`** — the modal command palette: pure state machine (filter / navigate / MRU cap 3 /
  picker) + renderer (translucent panel, grouped rows, hotkey column, scroll-to-selection,
  every text run clipped to the panel via `overlay::fit`, all width math saturating).
- **`main.rs`** — the ~300-line hotkey if-chain is now data: one binding scan over the registry +
  one dispatch `match`, with every old handler body moved verbatim (audited word-for-word,
  three ways, by an adversarial review). Palette wired: backtick or Ctrl+P opens; typing filters;
  Enter runs; Esc (or backtick again) closes.

Gates at merge: fmt clean, clippy `-D warnings` 0 in both feature variants, frontend 123/96
tests, **full workspace suite EXIT=0 (36 legs, 0 failures)**, `crates/oracle-core/` a zero-byte
diff for the whole branch. Zero currency movement by construction.

## ⚠ DEFAULT-SEMANTICS CHANGES (owner: read before your first session)

1. **Esc no longer quits.** Esc closes the palette/picker; quit = window close button or the
   `Quit` palette command. The old Esc-quit reflex will close nothing.
2. **Tab now soft-resets** (previously inert). Gens/Fusion muscle memory honored per the
   brainstorm; F1 still works. SRAM is preserved and a reset is reversible, but a stray Tab
   mid-run restarts the machine.
3. **After running a palette command, the keyboard reaches the game only once every key is
   released** (the close-latch that stops Enter leaking Start into the game). A brief
   held-key dead moment right after a palette action is this working, not a bug.

## ☐ OWNER-OWED: the windowed smoke test (plan Task 6 Step 8) has NOT been run — headless here

`cargo run --release -p oracle-frontend -- <rom>` and walk the checklist: startup toast;
backtick opens the grouped list; typing filters; Enter on "Pause / resume" pauses; the slot
picker (arrows + Enter, occupancy labels); all old hotkeys unchanged; Tab AND F1 reset; Esc
closes the palette and does NOT quit; arrows/A/S/D dead while the palette is open **and click
on the panel arms no watch**; Quit command exits. Also joins the standing owed list: gamepad
deadzone (0.5 unfelt), SY-7 mix levels.

## Review-loop record (what the two-stage + final process caught this run)

Per-task reviews caught and fixed: an unjustified lint suppression (Task 1); a vacuous MRU
mutation → strengthened test, and a silent hidden-row leak on the query branch → self-validating
test (Task 4); the sel-on-header first-frame invariant break and stale picker query (Task 4,
plan's own code); a long-query containment escape and a narrow-panel subtract-overflow panic
(release-mode hang) in the renderer (Task 5, plan's own code); the lost `SLOT_COUNT`
compile-time tie (divergence proven to fail the build), a rustfmt-abandoned block, and palette
mouse click-through (Task 6). The **final whole-branch review** then found what per-task review
structurally cannot: the Enter→Start close leak and the below-the-fold invisible selection —
both now fixed and pinned by mutation-verified tests. Every evidence-bearing test in the slice
carries a recorded mutation line in its commit body.

## Registered follow-ups (not built, with anchors)

- **F-PALETTE-SCROLL** — full scroll UI: truncation indicator, page keys; and the *picker* list
  still paints top-down without scroll (clips only at pathological geometry). Anchors:
  `palette.rs` draw break vs `move_sel`, and the picker branch of `draw`.
- **F-PALETTE-HINT** — startup hint string is hardcoded; spec §4 wants it derived from the
  registry (the open key isn't a registry row, so this needs a decision, not just a refactor).
- Recorded behavior deltas (accepted): same-frame multi-key presses now toast per command (old
  chain coalesced); same-frame ordering follows registry order; holding Ctrl+P past the OS
  repeat threshold types `p` into the query.

## Next

S2 (config file + persistence) per spec §7 — planned and started overnight if time allowed;
see the memory file and any `docs/superpowers/plans/2026-08-17-player-s2-*.md`.
