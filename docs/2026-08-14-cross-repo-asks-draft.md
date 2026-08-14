# Two cross-repo asks — DRAFTED, NOT SENT (2026-08-14)

**Status: BOTH ACTIONED, 2026-08-14** (Fable ruling E). The framing this document opens with was wrong,
and the ruling says so plainly: there is no "other side". One person owns every repo, so these are work
items in the owner's own suite, not petitions. What survives the correction is the *sequencing*
discipline — write the contract first, implement second — not the separation of parties.

- **Ask 1 — DONE.** It collapsed into CR-6. Aurora's spec corrected (`aurora` commit `26378c9`); the
  contract's camelCase spelling made normative with both-spellings bridging explicitly forbidden
  (`empyrean` commit `3b49e1a`, `protocol.md` §3 and §10 decision 4).
- **Ask 2 — RECORDED, NOT IMPLEMENTED.** The requirement is now tracked in `empyrean/docs/ROADMAP.md`
  (Sigil greenfield track) with its minimum field set, and `protocol.md` §9 splits the manifest out from
  the deferred build node — the manifest is a producer-side artifact that needs nothing from Aether to
  exist. **The sigil-side implementation is unstarted**, deliberately: writing code in the assembler that
  builds the game was not in scope for the contract pass.

*Original framing, kept for the record:* both of these are change requests against repos oracle-next does
not own (`empyrean/contract`, `sigil`, `aurora`). Filing them is outward-facing and is the owner's
call, not mine — so they are written out here ready to send, and nothing has been sent, filed, or
committed to another repo. Both were surfaced by the 2026-08-13/14 engine-side recon
(`docs/2026-08-14-tooling-frontier-recon.md` §6).

Neither blocks the current overnight arc. Both are cheap for the other side.

*Figures below re-verified firsthand on adoption:* `aeon/s4.lst` has `Camera_X : FFFFA428` (it has indeed
moved twice more since the contract's D7 incident was written up), and the demo pair share an identical
`EndOfRom : 11224` while **1,197** shared symbol names sit at differing addresses — so the `deb2`
appendix probe genuinely cannot separate them, exactly as claimed.

---

## Ask 1 — resolve the `rom_reloaded` vs `romReloaded` event-name drift

**Severity: low, but it is a live latent bug and it gets more expensive the longer both spellings
exist in approved documents.**

- `empyrean/contract/protocol.md` §3 specifies the push event as **`emulator/romReloaded`** (camelCase,
  consistent with `protocolVersion` and the rest of the envelope).
- `aurora/docs/specs/2026-07-03-aether-client-playtest-design.md` — an **approved** spec — subscribes
  to **`emulator/rom_reloaded`** (snake_case).

One of the two is wrong. Since the contract explicitly declares itself the source of truth (*"the
emulator conforms to it, not the reverse"*), the presumption should be that Aurora's spec needs the
correction — but that is the contract owner's call, not oracle-next's.

**What we are asking for:** a ruling on which spelling is normative, and a correction to whichever
document is wrong.

**What we are explicitly NOT doing, and why:** oracle-next will not quietly emit both spellings. A
server that accepts both makes the drift permanent and invisible — every client keeps working, so
nobody ever fixes it, and the next client author copies whichever one they happened to read. We would
rather implement the one true name and have Aurora's subscription fail loudly than paper over a
contract disagreement in our implementation.

**Cost to the other side:** a one-line edit in one document.

---

## Ask 2 — ship `s4.build.json` from `sigil build`

**Severity: moderate. This is the standing fix for the single most-cited bug class in the suite docs.**

Stale symbols have now bitten **three times**. The contract itself documents an incident where
`Camera_X` moved `$FFFFA120` → `$FFFFA144` mid-session and every symbol shifted by +$24, rotting a
"verified" literal inside a single session (`protocol.md:68-78`). That documentation is itself out of
date in an instructive way: today's `aeon/s4.lst` has `Camera_X : FFFFA428`, so **it has moved twice
more since the incident was written up**.

A build manifest is already specified and already blessed — it simply has not been emitted:
- `empyrean/docs/STUDIO_VISION.md` §6.2 specifies it.
- `empyrean/docs/ASSEMBLER_VISION.md:159` references it.
- `empyrean/CLAUDE.md:203` says to ship it **"now"** rather than waiting for Sigil Spec 4.
- The sibling emulator already carries a no-op validation seam for it (`protocol.md` §4 forward note,
  §8 item 7), and its `buildId` check is stubbed out (`ControlSocket.cpp:1487-1492`).

**What we are asking for:** `sigil build` emits a small `<rom>.build.json` alongside the `.bin` and
`.lst`, carrying at minimum a build id, the ROM hash, the game and shape (`s4` vs `s4.debug` vs `demo`
— these have *different RAM layouts*, so shape is not cosmetic), and the paths/hashes of the emitted
artifacts.

**Why it matters specifically to oracle-next:** we can then refuse a symbol table that does not belong
to the loaded ROM, instead of cheerfully resolving every address to a plausible-looking wrong name.
A wrong symbol is worse than no symbol — no symbol makes you go look it up; a wrong one makes you
confident.

**Partial mitigation already built, requiring nothing from anyone — and now measured.** Every shipped
Aeon ROM carries a `deb2` symbol appendix at `EndOfRom`. Verified to the byte on 2026-08-14: `s4.bin`
is 696,836 bytes, the appendix starts at `0xA11F0` (659,952), the difference is **36,884**, and the
magic `de b2 04 02` is exactly there. oracle-next now uses that as an interim build check
(`crates/oracle-core/src/symbols.rs::validate_against_rom`, shipped `642d77e`).

**But it is a filter, not a proof, and we have the counterexample.** `demo.lst` and `demo.debug.lst`
both declare `EndOfRom : 11224` — identical — while sharing **1,197 symbols at differing addresses**.
The appendix probe cannot separate that pair, which is why `validate_against_rom` is deliberately
three-state (`Match` documented as "not obviously wrong", never "verified"). The s4 shape crosses are
caught; the demo pair is not.

Every alternative we could check from the consumer side was checked and rejected: the ROM header
`$100–$18D` is **byte-identical** between s4 shapes (it separates games, never shapes); the `.lst`
carries no date, version or hash; symbol names are not ASCII-searchable in the appendix; and while
`$1A4 == len-1` holds 5/5 and `$18E` is a real whole-image checksum, both validate the ROM against
*itself* and say nothing about which `.lst` belongs to it.

**So the clean fix is producer-side, and it is small:** have `sigil build` emit the built image's
checksum (and the shape) into a sidecar beside the `.lst`, or into the manifest this ask is already
about. That converts a three-state guess into a decision.

**Cost to the other side:** small, and it is already on their own roadmap with a "ship it now" note
against it.

---

## Suggested handling

Both are better raised as short written change requests against `empyrean/contract` than as informal
mentions, since the contract's whole value is that it is the written source of truth. Ask 1 is a
one-line correction and could go in immediately. Ask 2 is a small feature on sigil's side and can
follow at whatever pace suits, since we have the `deb2` fingerprint as an interim.
