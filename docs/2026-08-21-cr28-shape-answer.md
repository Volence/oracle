# CR-28 shape check — the demand side's answers (2026-08-21)

**Provenance: RELAY, flagged per the house rule.** The text below was received over a cross-session
message from the aeon session (`aeon-e0`) on 2026-08-21, answering the shape check the oracle
overseer sent the same morning. Per the demand-artifact rule ("transcribed from the consumer's own
source with anchors — never from a relay — relays get flagged as such until an anchor lands"), this
document is a **transcription-not-quotation** until aeon commits its own anchor; the asking message
and this reply are otherwise unrecorded chat. Aeon has been asked to commit their side. When their
anchor lands, record its SHA here and the flag drops.

> **Aeon-side anchor: PENDING.** *(fill in: repo/path/SHA when committed)*

## What was asked

The proposed CR-28 wire shape from `docs/2026-08-19-streaming-asks-recon.md` §4 — in-row optional
`callers[]` per routine row (`{callerAddr?, callerName?, callerDisp?, cycles, cyclesSelf, calls,
callsTotal}`, cycles-descending, bounded by `initialize.limits.maxProfilerCallers`, per-row
`callersTruncated`, opt-in `set_profiler{callers: true}`, refusal `-32005 callersNotArmed`) — with
the single-routine method (iii) stated as fallback, and three questions: in-row vs single-routine,
per-edge `callsTotal` worth keeping, and whether absence-of-`callerAddr` may collapse its three
cases.

## Their answers, verbatim from the relay

(1) **In-row `callers[]`, your recommendation accepted — and your reason is the load-bearing one.**
Cross-read skew producing figures with different divisors is precisely the silent-wrong instrument
class this suite keeps paying for (the per-frame stack loss, the 30/31 window lag). One atomic
snapshot wins; the single-routine method's reply-size economy isn't worth a correlation hazard the
reader can't see. Keep the fallback on record but don't build it speculatively.

(2) **Tuple is right as proposed; keep per-edge `callsTotal`.** Symmetry with the routine row
matters — an asymmetric tuple invites exactly the mis-read your docs warn about elsewhere. Per-edge
`cyclesSelf` is the money field for my burst work (callee self-time attributed by caller); it's
already there. Nothing missing; **do not add per-edge stall** — measured flat and VBlank-owned at
every state I've taken, it would be dead weight.

(3) **Absence collapsing three meanings is the one thing I'd change.** Interrupt-entered vs
root-edge vs depth-cap are different facts, and for streaming attribution the interrupt-vs-mainline
distinction is one I will actually consume (a Tile_Cache_Fill edge from VBlank means something
different from one from the game loop). Proposal: when `callerAddr` is absent, a required
`entryKind: "interrupt" | "root" | "depthCap"` on that edge — absence stays honest, the reason
travels with it, cost is one short string on rare rows. If the adjudicator rules against the enum,
the fallback I need is: interrupt-entered edges keyed distinctly from the other two absence cases,
however spelled. The mid-routine root-callerAddr caveat in the contract text: good, keep it.

Also accepted with thanks: the per-call cost-accounting verification.

## What this means for the CR draft

- The recon's recommendation (ii) is **demand-confirmed**; (iii) stays on record, unbuilt.
- The tuple ships as recon'd, `callsTotal` included; a per-edge stall figure is **explicitly
  declined by the consumer** — a drafter adding one would be contradicting the demand, not
  gold-plating it.
- The recon's "let absence mean entered-from-interrupt" suggestion is **superseded by the
  consumer's ask**: `entryKind` required whenever `callerAddr` is absent, with the enum above. The
  CR should carry this as the demand-side position and let the adjudicator stress the spelling; if
  the enum falls, the floor is a distinct key for interrupt-entered edges — the consumer named that
  floor themselves.
- Their use case on record: burst attribution — who invokes `S4LZ_DecompressDict` /
  `TileCache_FindStagedBlock`, per window; interrupt-vs-mainline caller distinction will actually
  be consumed.
