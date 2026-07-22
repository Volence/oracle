# Emulator-side diagnostic tooling — idea capture 2026-07-20

Status: **idea capture**, not committed design. Filed here because oracle-next's charter
puts "deep debug introspection, designed in rather than bolted on" as a core axis — these
are the bus-level diagnostics that only the emulator can provide. Several are worth
**backporting to current Oracle (Exodus)** now, since it's the daily driver; flagged below.

## Framing (shared across three repos)

We use vladikcomper's Error Handler/Debugger (+ `convsym`) as our one significant
not-from-scratch tool. It's a *drop-in library*: it runs **on the 68K itself** (an
exception vector — it does not intercept buses), renders crashes to the Genesis screen,
symbolizes PC → nearest label, and is post-mortem-only and 68K-only. That design is forced
by the constraints of a generic user: arbitrary emulator, no control over the assembler.

**We have neither constraint.** We own the whole stack (sigil + Oracle + MCP + build), and
we have **no real hardware**, so "the emulator is our hardware-accuracy substitute" is a
first-class goal. The high-leverage diagnostics are therefore the ones that are
*structurally impossible* as a drop-in library because they require seeing every bus access
— i.e. they live here, in the emulator, not on the target.

On-target (drop-in-tier) pieces live in `aeon/docs/DEFERRED_WORK.md`; assembler-side pieces
in `sigil/docs/2026-07-20-diagnostic-instrumentation-ideas.md`.

## Ideas (bus-level; emulator-only)

- **Bus-legality / hardware-illegality detector (the big one for us).** Flag every access
  that works in emulation but would be unreliable/illegal on real hardware: reading VRAM
  without stopping the Z80, VDP writes during active display, 68K touching the Z80 bus
  without holding it, DMA overrunning VBlank, VSRAM/CRAM writes at illegal dot timing.
  Because our standing constraint is *no real hardware*, this is the closest thing to a
  hardware test lab we can ever have — it converts "works in the emulator" into "would work
  on a Genesis." Highest strategic value; impossible as a drop-in library.

- **VRAM/CRAM/VSRAM watchpoints — "who wrote this tile?"** Break when anything (CPU *or*
  DMA) writes a given VDP address. Kills more graphics bugs than everything else combined,
  and no on-target library can do it because the write lands inside the VDP, invisible to
  the 68K. **Backport candidate for current Oracle** — very high value now.

- **Structured crash-frame reader (pairs with the aeon mailbox).** When the target writes a
  fixed crash-frame struct to a known RAM address and halts, Oracle reads it off the MCP
  socket as structured data (regs/PC/SR/fault addr/breadcrumbs) instead of screenshotting a
  register dump. **Backport candidate for current Oracle.** Cheap on the emulator side.

- **Provenance / taint tracking.** Tag a byte, follow it through registers and memory:
  "this corrupt `art_tile` — where did the value originate?" Root cause instead of symptom.

- **Instruction / byte execution coverage.** Which code actually ran — dead-code detection
  and real test coverage for assembly. Feeds the golden self-test suite.

- **Deterministic record + input log, rewind-to-cause.** Log inputs, reproduce any crash
  exactly, then walk backward from the deathbed to the corruption (an address error
  typically fires several instructions *after* the real fault). Only possible because we own
  the emulator. Note the existing determinism caveat: VGM capture is realtime-only.

- **Real call-stack reconstruction.** 68K has no frame-pointer convention, so the drop-in
  handler's backtrace is heuristic (scans the stack for return-address-shaped words). If
  sigil tags call sites (see the sigil note), Oracle can give an exact backtrace.

## Recommendation

Two highest-leverage builds for *our* constraints: (1) the **bus-legality detector** —
because it's our only possible hardware-accuracy substitute and structurally can't be a
drop-in; (2) **VRAM watchpoints** — highest day-to-day payoff and a clean current-Oracle
backport. The crash-frame reader is a cheap companion to the aeon-side mailbox.
