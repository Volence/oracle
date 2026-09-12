# STATE-HASH-TYPED-MASK (lens M16 + M18): the CR-drafting brief

> **Dispatch artifact, 2026-09-12 (overseer, session 6).** The last row of LENS-WAVE-4
> (`docs/2026-09-11-lens-triage.md`, "Wave 4 and later" and the "Updated after wave 4" addendum). Contract-first:
> this agent drafts the change request ONLY; the code and the protocol amendment land together in one window after
> the hub's un-framed adjudication. Premises measured before dispatch, server at oracle `4a2865e`, contract at
> empyrean `origin/main` `e394fdb` (read by `git show`, never through the sibling path):
> `Engine::state_hash` at `crates/oracle-aether/src/engine.rs:5125` emits a CONSTANT caveat (`:5143`, "these
> fingerprints cover VDP state only …") on every reply and, with `includeFramebuffer` and a set layer mask, appends
> `masked_hash_caveat()` (`:3708`) to it (`:5174-5179`, which `expect`s the constant to be there). `read_vram`
> (`:4849`) emits a constant "debug read: …" caveat (`:4870`). `read_memory` (`:4451`) is ALREADY conditional
> (`region.caveat(addr)`, `:4464-4471`, since §11.48), and contract §2.4's advisory now says so in its own text.
> The vendored fragments (`crates/oracle-aether/tests/contract/bus-protocol.schema.json`) declare `caveat` as a bare
> string on both `emulator/state_hash` and `emulator/read_vram`; `state_hash`'s result has no typed mask key.
> Contract: §2.4 at `protocol.md:591` (rule 3: clients MUST NOT parse a caveat, and any consequence a client must
> act on needs its own typed key; the advisory: SHOULD prefer conditional caveats); §11.27 at `:4807` (its
> "MUST NOT emit unconditionally" is written about `pixel_attribution`); the `state_hash` §6 row at `:2107` and the
> `framebufferSource` paragraph at `:2140`. The last amendment is §11.48; the last CR letter in
> `docs/proposed/` is CR-R, and the contract's §11 carries CR-S, CR-T and CR-U after it.
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are a research-and-drafting agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Your deliverable is a **change request (CR)** to the suite contract, `empyrean/contract/protocol.md`, closing two findings from the 2026-09-06 lens sweep. **You change no code.** The code change lands after the contract owner (the hub) adjudicates your CR; your document is what they adjudicate and what the implementing agent then builds from, so it must stand up cold.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it. The last four agents on this lane each corrected their brief, and one corrected it on nine points. If a finding no longer holds at HEAD, say so in the CR and propose nothing for it.

## The two findings (packet: `docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md`, entries M16 and M18; triage rows in `docs/2026-09-11-lens-triage.md`)

- **M16.** `emulator/state_hash {includeFramebuffer: true}` deliberately hashes the UNMASKED picture (`LayerMask::ALL`) even while a display layer mask is hiding a plane. That is correct (a determinism fingerprint must not move because someone hid a layer). But the only thing on the wire saying "this hash is NOT the picture your screenshot shows" is text appended to a caveat, and §2.4 rule 3 forbids a client from parsing a caveat. So a client that hashes the framebuffer to pin what it is looking at has no legal way to learn it pinned a different picture. **The CR proposes the typed key.**
- **M18.** Methods emit caveats that are present on every reply. §2.4's advisory says a permanent property of a method belongs in the document, not in a caveat a client learns to ignore. The packet counted three sites; `read_memory` has since gone conditional (§11.48), so check whether it is two now: `state_hash`'s constant and `read_vram`'s constant. **Check the packet's claim that §11.27 makes this a MUST NOT**: my reading is that §11.27's MUST NOT is scoped to `pixel_attribution` and the general rule is §2.4's SHOULD. Settle it from the text and state the strength honestly in the CR; do not inherit the packet's stronger reading, and do not weaken a real MUST either.

## What the CR must answer

1. **The typed key for M16.** Name, type, and presence rule (when it appears, when it is absent, how it relates to `framebuffer` and `framebufferSource`). Consider at least: a boolean; the list of hidden layer names, using the SAME spellings the bus already uses for layers (find them: `set_layer_enabled` / `get_layer_states` and `masked_layer_names`); and anything better you find. **Run a visible better-approach pass** (house rule: an existing shape is the floor, never the ceiling). That includes asking whether the hash should be offered for the masked picture as well, and saying why not if not.
2. **The same fact on the sibling methods.** `emulator/screenshot` and `emulator/scanlines` carry the mirror half of this divergence as caveat text (`Engine::mask_caveat`, the doc comment beside `masked_hash_caveat` explains the pairing). If the typed key belongs on those too, say so, but as a **separable item** the adjudicator can take or leave, so the M16 core is not held hostage to a wider change.
3. **The constant caveats (M18).** For each remaining unconditional site: what the permanent property is, whether §6 or the fragment's `$comment` already states it (the `state_hash` fragment's `$comment` already says "The five fingerprints cover VDP state only"), and what text the contract should gain so nothing a reader needs is lost when the caveat goes. After the change, `state_hash`'s caveat should be conditional (present only when it has something to say), so work out what it says and when.
4. **The consumer set, in three bins: BRANCH, PASS-THROUGH and DISPLAY.** A consumer that shows a caveat or a key on screen is a consumer (the display bin is the one a branch-only sweep misses; `OVERSEER.md`'s F-SERVERNAME section is the worked example). Sweep every sibling repo at its **pushed** branch, not its working tree: `git -C /home/volence/sonic_hacks/<repo> grep -n <pattern> origin/<main-or-master>` over aeon, sigil, aurora, seraph, empyrean and dominion (find each repo's default branch; oracle-old is reference-only, sweep it but mark it so). Exclude worktree copies and vendored trees. Vary the spelling: the quoted method name, `state_hash`/`stateHash`/`state-hash`, the MCP tool names (`emulator_state_hash`, `emulator_read_vram`), and any key name you propose. Then this repo's own consumers: the player window's panels, tests that pin either caveat's text or presence (there is at least one `expect` in `engine.rs` itself), examples, and docs that quote the text. State which bins you ran and what each found, including zero results with the positive control that shows the grep could see a hit.
5. **Classification (the protocol's bar 14 corollary).** List every wire-visible change: key added, caveat removed from which replies, caveat text changed. For each, say whether any consumer must change in lockstep or whether it is additive. Say plainly that no hash value moves (the FNV layout is a suite contract; `crates/oracle-core/src/state_hash.rs` and its doc). If something does move, stop and report it: that is out of scope.
6. **Vectors.** Propose schema vectors (valid and red) in the CR-G shape, and **run them against the schema before handing over**: take the contract's `contract/schema/bus-protocol.schema.json` and `contract/schema/tests/validate_contract_schema.py` from empyrean `origin/main` via `git show` into your scratch dir, patch your proposed fragment into the scratch copy, and show each vector's verdict. **Only propose as a vector what a schema can actually judge.** §11.27's own correction paragraph records two proposed vectors that were not expressible as schema documents; anything like "present exactly when a mask is set" is a live conformance row on the server's wire, so list those separately as conformance rows for the implementation.
7. **Implementation sketch** for the post-adjudication parcel: the files and functions that change (engine handler, fragment re-vendor, tests), the tests that pin the new rule, and the stale comments that go (for example, `crates/oracle-aether/tests/pixel_attribution.rs` near line 733 is said to still call `read_memory`'s caveat "the in-tree example" of the forbidden shape; check it). Keep this short; it is a map, not a patch.

## Shape of the document

Copy the shape of `docs/proposed/2026-09-05-cr-r-vdp-register-readback.md` (read its head): title line; **Raised by**; **Target** (the next free `§11.x` and whether §6 rows and §2.4 are amended); **Reviewer: to be named by the adjudicator in the ruling itself** (the substituted-reviewer rule, `OVERSEER.md` "HUB RULING UNDER DELEGATION, 2026-08-27"); and **Revisions everything below was read at** (the empyrean `origin/main` SHA you read, printed by `git rev-parse`, and oracle's HEAD). Then the evidence, the proposal with its alternatives and why each lost, the consumer sweep, the classification, the vectors with their measured verdicts, conformance rows, and the implementation sketch. Name the CR with the next free letter. Find it by reading every CR letter the contract's §11 headings and body use at `origin/main`, and every CR file under this repo's `docs/proposed/`. Say how you found it.

Write for a reader who has never seen this repo: the adjudicator reads it cold, and so will the model that audits unadjudicated decisions later.

## Deliverables

- `docs/proposed/2026-09-12-cr-<letter>-state-hash-typed-mask.md`
- `docs/proposed/2026-09-12-cr-<letter>-vectors.json` (if you propose vectors), in the shape of `docs/proposed/2026-08-30-cr-g-vectors.json`.

## 0. Base check, branch, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 4a2865e HEAD && echo OK` must print OK, and `docs/2026-09-12-state-hash-typed-mask-cr-brief.md` must exist. Otherwise BLOCKED, stop.
- `git switch -c parcel/state-hash-typed-mask-cr`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no.
- `ls` is aliased to eza; use `command ls`. Never add `2>/dev/null` to a command whose emptiness you will report as a finding.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/state-hash-cr-$(date +%s)/`.
- Read the contract ONLY as `git -C /home/volence/sonic_hacks/empyrean fetch -q origin && git -C /home/volence/sonic_hacks/empyrean show origin/main:<path>`. The sibling directory is another session's live working tree.

## Hard boundaries

- **Files you may create or edit:** the two deliverables above. Nothing under `crates/`, `tools/` or `vendor/`. **Never touch `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl` or any `docs/OVERSEER*.md`**: those are the controller's files, and the first is parsed by a console you cannot see.
- **No cargo is needed**, and none should run: another session may be running cargo in this repo, and two concurrent cargo runs here produce spurious failures. If you find you truly need a build to answer a question, STOP on that question and report it as a question for me.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool: they deadlock from background agents, and the only live emulator on this machine may be the owner's own window. Anything that wants a runtime confirmation is TAGGED in the CR as a conformance row for the implementation parcel, not attempted.
- **Never write to a sibling repo.** Read-only `git grep`/`git show` against their `origin` refs only.
- **Exact-path commits:** `git add <path>` for each file, never `-A` or a glob; check each commit with `git show --stat`. Commit as each piece lands, and put what you found into the commit message: if you die mid-task, your commit bodies are what your successor reads.
- **BLOCKED is always available.** If a constraint seems to force a worse design (for example, the best key would need a hash to move, or a consumer would break in lockstep), stop on that item, record exactly why, and finish the rest. Never quietly narrow the proposal to make it easier to adjudicate.

## Report (your final message)

1. **Where the brief was wrong**, first.
2. Branch name, tip SHA, and `git show --stat` of each commit.
3. The proposal in five lines: the key (name, type, presence rule), what happens to each constant caveat, and the separable item if any.
4. The consumer sweep: bins run, repos and refs swept, hits per bin, and the positive control.
5. The vectors, each with the verdict you measured (not the one you expect).
6. Anything you could not settle, and why.
