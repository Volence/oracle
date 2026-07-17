# BlastEm frame-capture feasibility spike (VDP push 5, timeboxed)

**Question.** Can BlastEm 0.6.2 be driven headlessly to emit a framebuffer image for a fixture ROM, to upgrade
a golden-frame self-consistency pin (`crates/oracle-core/tests/golden_frames.rs`) to a cross-emulator
confirmation? Clean-room: BlastEm is a **black-box** screenshot producer here — no source is read (only its
shipped `default.cfg`, a config file, was inspected).

**Result: not achievable within the timebox with the available tooling.** The goldens stand as
self-consistency pins; the s4.bin/Exodus golden-frame rung (design §5 rung 2, a real ROM) remains the eventual
cross-oracle confirmation. This is a recorded negative result, not a blocker — push 5 does not depend on it.

## What was tried

- **Environment:** `blastem64-0.6.2` at `emulators/blastem64-0.6.2/blastem`; `xvfb-run` present; ImageMagick
  `import`/`convert` present; **`xdotool` absent**; no window manager on the virtual display.
- **BlastEm's screenshot mechanism** (from `default.cfg`): bound to the **`p` key** (`ui.screenshot`), written
  to `$HOME/blastem_%Y%m%d_%H%M%S.png`. There is **no command-line switch** to take a screenshot or run headless
  (`--help`/`-x` are unrecognized switches; BlastEm parses only its documented flags).
- **Attempt:** ran `blastem <rom>` under `xvfb-run -s "-screen 0 640x480x24"`, slept 6 s, then
  `import -window root out.png`. `import` succeeded but produced a **blank 640×480 1-bit-grayscale PNG** (176
  bytes) — the BlastEm SDL window was not present on the X root.

## Why it did not yield a usable capture

Two independent gaps, each of which alone blocks the path with the installed tooling:

1. **Screenshot-key route needs keypress injection.** BlastEm's own PNG screenshot is a keypress (`p`).
   Injecting it headlessly needs `xdotool` (or equivalent) — **not installed**.
2. **Root-window grab needs the SDL window mapped to root.** With no window manager on the Xvfb display,
   BlastEm's SDL window is not composited onto the root, so `import -window root` grabs a blank root. Targeting
   BlastEm's own window instead needs its window id (via `xdotool`/`wmctrl`/`xwininfo`) — **not installed** — or
   a WM to reparent/map it.

Closing either gap (install `xdotool` + a lightweight WM, or drive the `p` key via injected input) is a
plausible follow-up but is beyond this bounded spike and would not be reproducible on a clean CI box without
those dependencies.

## Recommendation

- **Do not block push 5 on this.** The golden frames are honest self-consistency pins (they lock the current
  model against silent drift); the pixel known-differences ledger (`docs/2026-07-16-vdp-pixel-known-differences.md`)
  enumerates exactly what a future differential must attribute.
- **If a cross-emulator frame oracle is later wanted**, the cheapest route is the RSP-stub path already proven
  for the register differential (`rsp.py`): drive BlastEm to a chosen frame boundary, then read VRAM/CRAM/VSRAM
  over the stub and render *those bytes* through our own `render_line` — comparing **decoded state**, not pixels.
  That sidesteps the framebuffer-capture problem entirely and stays in the clean-room black-box contract. The
  true pixel oracle (DAC-accurate RGB, P8) still wants Exodus/hardware on `s4.bin`, which is the design's rung 2.
