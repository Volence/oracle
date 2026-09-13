# Brief: OVERSEER-CUT — cut `docs/OVERSEER.md` to about 40 KB by WHEN each rule is read

**First, before any edit: tell me where this brief is wrong.** Five consecutive agents in this repo have corrected
their brief on a material point, measured. Put your disagreements at the top of your report.

## The go, and what it covers

Owner, verbatim, 2026-09-13T21:53:38Z, heard by the hub, banked at empyrean `492a2ac:docs/OVERSEER.md` lines 78-83
(verified ancestor of empyrean `origin/main`): *"cut sounds good."* It answered a proposal naming this exact job:
**each lane cuts its own `docs/OVERSEER.md` to about 40 KB by the when-read rule, aeon's ~20 KB boot file as the model.**
The method is his 2026-09-04 ruling (*"7. Sounds fine"*): **split BY WHEN A RULE IS READ, never by size; nothing
rewritten; proven lossless.** Read `git -C ../empyrean show origin/main:docs/OVERSEER-PROTOCOL.md`, section
"The boot read is bounded", for the governing text, and `git -C ../aeon show origin/master:docs/OVERSEER.md` for the model
shape (read it; do not copy its content).

## Starting point (measure it yourself; these are mine at `3b6af99`)

`docs/OVERSEER.md` 97,771 B, 1,114 lines. `docs/OVERSEER-REFERENCE.md` 93,005 B. `docs/OVERSEER-LOG.md` 323,897 B.

## Where things go — classify by CONTENT, never by heading

- **STAYS in `OVERSEER.md` (the boot read):** what a fresh session needs to act AT BOOT. Scope and role; the queue and
  "Order of work"; any resume brief; the standing rulings that change what a session does FIRST or at EVERY stop
  (cut-the-ceremony, report-to-the-hub, say-when-you-need-a-clear); the **Bootstrap stanza** (it MUST stay: the protocol's
  preamble sanctions exactly it, and a rule filed later cannot protect a read already done); "Where the detail lives".
- **To `OVERSEER-REFERENCE.md`:** LIVE rules read at ONE later moment. Examples, yours to confirm or refute: push
  authorization and its conditions (read before pushing); the follow-up register's live bookings (read when picking work
  or touching that area); CR/contract rulings (read before a CR or a wire change); the ORACLE-DEBUG-UI tab ruling and d-25
  (read before a panel parcel); the lens-count reporting rule (read before reporting lens numbers); the UX seat pair (read
  before running a UX lens); `run_to`/`resume` wire rules; LAYER-MASK; coordination notes.
- **To `OVERSEER-LOG.md`:** CLOSED history only, append-only at the end. A live ruling NEVER goes only to the log.
- **A new file is allowed only if it has a distinct when-read trigger** that neither of those two fits. Name it in
  "Where the detail lives" and declare it to the proof tool.
- **Each moved section leaves its exact original heading line plus ONE pointer line** in `OVERSEER.md`:
  `Moved whole to docs/<file> (same heading); read it <when>.` Code comments and other lanes cite sections of this file
  BY NAME (e.g. `crates/oracle-aether/build.rs:158`, `crates/oracle-frontend/src/lib.rs:6`, `oracle-player/src/ui.rs:578`);
  the stub keeps every such citation landing on a heading that says where the text went.
- **Nothing is reworded.** Blocks move whole and byte-identical. If a block cannot move without editing a sentence, it
  stays, and you report it. **"About 40 KB" is a target, not a licence:** if reaching it would move something a session
  needs to act at boot, stop at the honest number and report the residual with what each remaining block is (protocol
  "measure-then-move" rule 3: that residual goes to the owner, never into a trim).

## The proof — this file's own section "The boot read is bounded" carries the traps; read it first

Tool: `tools/prove_doc_split.py`, run from the repo root. **Two traps already paid for here:**
1. **Do NOT declare `OVERSEER-LOG.md` or `OVERSEER-REFERENCE.md` as `--output`.** Both already hold thousands of lines that
   were never in `OVERSEER.md`, so the tool reports them `NOT DECLARED` and exits 1 on an untouched tree. **Declare the
   appended SLICES instead:** write each moved block to its own scratch file, pass those as `--output`, then separately
   prove each slice is verbatim-contiguous in its destination (e.g. a Python `slice in dest_text` check, printed per slice).
2. **Run your exact invocation against the UNMODIFIED tree FIRST and require it GREEN** before trusting any verdict after the
   cut. A red control makes the later verdict uninterpretable. Report both runs' exit codes and provenance lines.

Also: check the provenance line names the original's NON-BLANK line count; read BOTH PROOF 3 numbers (`--headings` gates the
verdict on one) and confirm the heading-blind seams land on headings; pointer lines and stub headings are NEW text, declared
via `--new <scratch file>`. **The follow-up register's list-shaped entries are not movable mid-list** (PROOF 3 refused one
before: `F-HOSTED-RESET-SRM` is a parenthetical inside a running comma-list); move a list whole or leave it.
**Cross-reference repairs are their OWN commit after the cut** (mixing them in stops the proof telling a move from a rewrite),
repointed by name, and sweep this file's own preamble and "Where the detail lives", which advertise sections that moved.

## Gates to keep green (debug profile is fine for these)

- `cargo test -p oracle-aether --test overseer_bound` — checks bytes AND content (it requires the text to name
  `OVERSEER-REFERENCE.md` and `OVERSEER-LOG.md`; read the file before cutting so you know every assertion). If an
  assertion encodes the old size as a ratchet, report it; do not edit a gate to make the cut pass without saying so.
- `python3 tools/test_prove_doc_split.py` and `python3 tools/lane-check.py --decisions docs/decisions.jsonl --status docs/lane-status.json --log docs/lane-log.jsonl`.
- The full landing run (`tools/land.sh`) is the controller's, on the merged tree. Baseline there: release 90/90 legs, 2907 passed / 0 failed / 3 ignored.

## Out of scope

`lane-status.json`, `decisions.jsonl`, `lane-log.jsonl` (the controller's). Any other repo. Any code change except comment
repoints in the cross-reference commit, and only if a comment names a section that no longer exists by that name anywhere.

## Report shape

Disagreements with this brief first. Then: final `wc -c`/`wc -l` of all three files; a table of every moved block (original
line range, destination, when-read trigger, one-line reason); the control and real proof invocations, verbatim, with exit
codes, provenance lines, both PROOF 3 numbers, and the per-slice containment check; gates run with aggregate totals; commits
(SHA + one line each) and your tip SHA; anything left in the boot file that you think could move but did not, and why.
