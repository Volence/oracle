# 68000 reference documents

Clean-room *permissive-documentation* sources for the m68000 core (per the audit clean-room
policy). We ground cycle counts, bus streams, and exception semantics on these — never on any
emulator's source.

## `Yacht.txt` — tracked

Yet Another Cycle Hunting Table, v1.1 (flamewing). Per-instruction cycle-accurate **bus-stream**
timing (`n`/`np`/`ns`/`nS`/`nV`/`nv` micro-cycle notation). Freely redistributed community
document, so it lives in the repo.

- Source: <https://gist.github.com/flamewing/af3b0863570afe317518eff849f58689>
- Notation writeup: <https://beyondbrown.mooo.com/post/yacht/>

## `M68000UM.pdf` — **untracked** (see `.gitignore`)

NXP MC68000 User's Manual (189 pp). The authority for **behavioral semantics** (exception
processing, the stacked-frame layout, STOP/RTE, privilege) and the exception-processing cycle
tables. Copyrighted 2.3 MB blob — fetch locally, do not commit:

```sh
curl -L -o docs/reference/M68000UM.pdf \
  https://www.nxp.com/docs/en/reference-manual/MC68000UM.pdf
```

Read it with the `Read` tool's `pages` parameter (it is a PDF). The exception-processing
chapter (§6) covers the frame layout, vector assignments, and per-exception timing; the
instruction chapter (§4/appendix) has STOP, RTE, and the privileged-instruction list.

## Which document settles what

- **Timing / bus-stream idle structure** → Yacht.txt (tiebreak: BlastEm-over-the-bus).
- **Behavioral correctness** (stacked PC, T-bit interaction, SR masking, vector numbers) →
  M68000UM. These are **never** xfail-manifest candidates — a wrong stacked PC resumes the guest
  at the wrong address and no current test would catch it (SST has no exception files).
