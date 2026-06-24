# Research digest — evidence base for the charter

Condensed from work done 2026-06-24 (in the Oracle session): an emulator-landscape
study, an Exodus core/shell separability investigation, two live spikes (BlastEm,
Ares), and a 5-angle from-scratch feasibility workflow. All claims here were
source-verified or live-tested; see each section for confidence.

## 1. The four axes, and the core finding

We grade an emulator foundation on: **debug introspection**, **hardware accuracy**,
**clean modern codebase**, **speed/headless-parallel**. **No single open Genesis core is
best-in-class on all four** — the assets split, which is what forced the build-fresh
decision.

| Core | Introspection | Accuracy | Modern code | Speed/headless | License | Maintained |
|---|---|---|---|---|---|---|
| Exodus | ★ unmatched VDP / per-pixel attribution | ★ VDP; CPU opcode-granular | ✗ Win32/DirectX heritage | ✗ no true headless | MIT | ✗ dormant since 2/2024 |
| BlastEm | ✓ dual-CPU debugger | ★ passes VDP FIFO test | ~ plain C | ★ headless `-b` | GPL-3 | ★ active 2026 |
| Ares | ~ tracers/regions; no MD breakpoints | ★ high | ★ C++20, separable | partial (headless via embed) | ISC | ★ pushed daily |
| Genesis Plus GX | ~ rich only in BizHawk fork | ✓ high | ✓ clean C | ★ proven parallel | ✗ non-commercial | ✓ active |

## 2. Exodus separability (verdict: HIGH, 7.5/10)

The Exodus device cores (~32k LOC) are **~70–75% pure emulation logic** with **zero
Windows API calls** in the hot paths; the linux-port already runs them headless-ish via
direct instantiation + POSIX shims. The 68000 (~16.2k LOC) and Z80 (~10.1k LOC) cores
are **MIT-licensed and already in the Oracle repo** (`Devices/M68000`, `Devices/Z80`) —
the bootstrap source for this project's CPUs.

## 3. BlastEm spike (live, nightly 0.6.3-pre)

- Headless `-b` exists but is one-shot; **headless + interactive works only via the GDB
  stub** (`-D -b`). Native debugger needs a display (xvfb).
- Deterministic N-frame advance: **YES** (vint breakpoint + continue, verified).
- VRAM/CRAM/VSRAM readable via the **native debugger only** (`vdp:vram[]` etc.); the GDB
  stub can't reach VDP memory (would need a small GPL patch).
- Conclusion: a fine **headless/CI backend** behind the bus protocol (the "hybrid"
  option), but not the foundation.

## 4. Ares spike (live, HEAD `6dc3f33`, 2026-06-24)

Compiled the core, wrote a ~90-line headless host, booted an MD ROM, ran 60 deterministic
frames, read 68000 regs + VRAM/CRAM/VSRAM, serialized **byte-identical** snapshots across
runs — binary links **zero** GUI/audio/X11.

- **Headless: proven.** Core links only libco/sljit/nall; `root->run()` = one
  deterministic frame.
- **Introspection present:** VRAM/CRAM/VSRAM + CPU/Z80 RAM as read/write nodes;
  instruction/interrupt/DMA/VDP-register tracers; registers via the CPU struct.
- **Determinism proven:** full-machine serialize → byte-identical (this is state_hash +
  savestate for free).
- **Missing for MD:** breakpoints/watchpoints, GDB stub wired to MD, decoded
  tile/sprite/plane viewer.
- **Effort:** minimal Oracle-lite backend ~2–4 days; full ~50-op parity ~2–4 weeks
  (dominated by breakpoints + stepping + viewers). ISC, builds in seconds.
- **The catch (why not the foundation):** libco cooperative-threading puts chip state on
  C stacks → fine-grained break/snapshot fights the architecture (byuu spent years on
  deterministic cooperative-thread savestates), and Ares's VDP still lacks FIFO timing.

## 5. From-scratch feasibility (5-angle synthesis)

**Verdict: viable and advisable for this owner** — ship a best-in-class agent-first
*debugger* core in months, then climb the accuracy ladder over years. The four axes are
**architectural intent**, achievable from day one; accuracy is the asymptote.

- **CPUs are the lowest risk** — instruction-accurate 68000 + Z80 are mechanically
  verifiable against SingleStepTests JSON suites + ZEXALL/ZEXDOC. **Cycle/bus-exact**
  68000 (microcode/PLA) is research-grade and **out of scope**.
- **The VDP is the cost center** — cycle/dot-accurate raster/FIFO/DMA timing is the
  multi-year tail (Exodus ~9 yrs to cycle-exact VDP; jgenesis ~2–2.5 yrs to pass
  VDPFIFOTesting). A **scanline renderer latching state at line start** covers ~99% of
  Sonic-hack content.
- **FM (YM2612) is the hardest chip** if hand-rolled — fully mitigated by **black-boxing
  `ymfm` (BSD)**; Nuked-OPN2 (LGPL) as an optional cycle-accurate backend + oracle,
  kept behind an isolated boundary.
- **Language/arch:** **Rust**, cycle-stepped explicit-state-machine chips, one `System`
  struct + central scheduler owning the clock + seed, everything Serde-serializable —
  which makes every cycle a valid break point and snapshot/hash/rewind near-free. (Zig is
  the runner-up; rejected for pre-1.0 churn + thin ecosystem + no borrow-checker.)
- **The accuracy-tail compression lever:** day-one **differential frame+register+RAM
  diffing** against BlastEm/GPGX/Exodus/Ares using Oracle's deterministic state-hash —
  earlier solo authors never had 3–4 accurate diff-oracles + a deterministic harness.

### License tiers for reuse

- **Copy-as-code (permissive):** Exodus 68000/Z80 (MIT, in-repo), Musashi (MIT, + Rust
  port `r68k`), Ares (ISC, adopt the separation pattern), floooh chips (MIT,
  cycle-stepped tick+pin template), tetanes-core (MIT/Apache, Rust trait shapes).
- **Study-only (do not copy):** BlastEm (GPL-3, best open VDP-timing reference),
  Genesis Plus GX (non-commercial, canonical scanline renderer), **jgenesis** (GPL-3,
  the closest modern from-scratch Rust analog — its CHANGELOG is a ready-made accuracy
  roadmap), fx68k Verilog + SpritesMind microcode threads (only if pursuing cycle-exact).
- **Black-box (link, don't fold in):** ymfm (BSD), Nuked-OPN2 (LGPL, isolated),
  Nuked-PSG (study the Sega LFSR taps).
- **Validation data (run, don't link):** SingleStepTests JSON (680x0/m68000/z80),
  ZEXALL/ZEXDOC, VDPFIFOTesting, Nemesis sprite-masking ROM, 240p Test Suite.
