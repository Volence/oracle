# CR-V SERVE (LENS-WAVE-4's remainder, M16 + M18): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 7).** Written from `docs/2026-09-12-lens-wave4-brief.md`'s shape.
> The ruling is empyrean `contract/protocol.md` §11.49 at **`0937079`** (ADOPTED WITH CHANGES; verified here an ancestor
> of empyrean `origin/main` `a81b273`, and a contract commit: protocol +115, schema +57, vectors +323). Premises
> re-measured at oracle `d55e331` before dispatch: `Engine::state_hash` at `crates/oracle-aether/src/engine.rs:5125`
> with the constant caveat at `:5143`; `masked_hash_caveat` `:3708`; `masked_layer_names` `:3675` (returns
> `self.layers.hidden()`); `read_vram` `:4849`; `scanlines` `:5755`; `screenshot` `:6920`. The vendored schema and
> vectors (`crates/oracle-aether/tests/contract/`, pin `a186e4b` in `PROVENANCE.md`) are blob-identical to empyrean
> `a186e4b`, `61d378b` and `30c10bc`, and `git diff --stat a186e4b 0937079 -- contract/schema/` is exactly CR-V's two
> files, so the re-vendor carries nothing but this ruling. `tests/scanlines.rs:346` already programs an H32 fixture.
> Baseline `tools/land.sh --no-push` at `f7c0cd9` (`d55e331` is docs-only on top): 89/89 legs, **release** 2858 passed
> / 0 failed / 3 ignored. S1 (the oracle-old MCP tool description) is NOT in this parcel: that tree has no remote and
> is booked separately. F-MASKED-ATTRIBUTION is NOT in this parcel. The text below the rule is the agent prompt,
> passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator + its Aether debug bus). Parcel: **CR-V SERVE**, the implementation of a contract amendment that has already been ruled. Size M. One branch.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it. The line numbers were measured at `d55e331` and will drift as you edit. The ruling's text outranks this brief everywhere they disagree, and the ruling's MUSTs outrank the CR draft everywhere those disagree.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor d55e331 HEAD && echo OK` must print OK and `docs/2026-09-12-cr-v-serve-brief.md` must exist; otherwise BLOCKED, stop.
- `git switch -c parcel/cr-v-serve`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza, so use `command ls`. In zsh, `"$R:contract/..."` is a history modifier and silently mangles the path: write `"${R}:contract/..."`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/cr-v-serve-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.
- **Another agent may be building in a sibling worktree at the same time** (a small player-UI parcel, file-disjoint from yours). The machine may be loaded. A test that fails only under load is a defect with a narrow window, not a flake: re-run it once to classify it, and report it by name either way.

## 1. What to read first (the sources of truth, in this order)

1. **The ruling:** `git -C /home/volence/sonic_hacks/empyrean show 0937079:contract/protocol.md`, section `### 11.49` (at the end of the file), plus the §6 paragraphs it added (grep that blob for `§11.49`: the `state_hash` coverage paragraph, the `displayMask` on `screenshot`/`scanlines` paragraph, the `emulator/read` peek bullet, §8 item 30). Read the **MUSTs M1-M4** and **SHOULDs S1-S2** closely.
2. **The CR as drafted:** `docs/proposed/2026-09-12-cr-v-state-hash-typed-mask.md`. §3 (what the key means), §7 (conformance rows CR1-CR9, which are YOUR tests) and §8 (the implementation map, written at `6b5b5c1`; re-find every site).
3. **The schema change:** `git -C /home/volence/sonic_hacks/empyrean diff a186e4b 0937079 -- contract/schema/`.

## 2. The serve (D1, D3, item B; R1 and R2 are schema/prose and cost you only tests)

- **D1, `emulator/state_hash`.** Delete the constant caveat. When `includeFramebuffer` is set, the reply carries `displayMask` (the hidden layers, `[]` when none; present **if and only if** `framebuffer` is), and a `caveat` **only** beside a non-empty `displayMask`, which is `masked_hash_caveat`'s sentence standing alone. With no framebuffer: no `displayMask`, no caveat. **`framebuffer` stays the UNMASKED picture** (§6 now says a server MUST NOT apply the mask to it). R1's `dependentRequired` also demands `framebufferSource` with `framebuffer`; confirm the server already sets both together, and say where.
- **D3 (M18), `emulator/read_vram`.** Stops emitting its constant caveat. Its `bytes` stay equal to `read{space:"vram"}`'s for the same range.
- **Item B + M4, `emulator/screenshot` and `emulator/scanlines`.** Both replies carry `displayMask` **always** (required by the schema now), `[]` when nothing is hidden, from the same derivation as `state_hash`'s. Do not change which `source` either one answers: the "masked implies `stateRender`" tie is a MAY, never a rule.
- **One derivation.** The wire's list, the player's badge (`LayerMask::hidden()`) and the vocabulary check must all come from one place. If `masked_layer_names` is already that place, use it; if a second list of layer names turns up anywhere, fold it into the owner and say where it was.
- **M2, check it and do not change it.** The ruling names the register bytes as *"the 24 VDP registers as bytes, the low 8 bits of each, which is what oracle-core folds"*. Read `crates/oracle-core/src/state_hash.rs` and its callers and confirm that is what is folded (report the lines). **If it is not, BLOCKED on M2**: do not change a hash to fit a sentence; report it and I take it to the hub.
- **No hash value moves anywhere.** Show it: the same fixture's five fingerprints and `framebuffer` before your change (at your base) and after, byte-identical, plus zero diff under every golden file.

## 3. The re-vendor, in the same branch

Re-vendor `crates/oracle-aether/tests/contract/bus-protocol.schema.json` and `vectors.json` **from `0937079`**, following `PROVENANCE.md`'s own recipe ("HOW TO PICK THE PIN": one revision for both files, verify each blob at that revision, check it is an ancestor of `origin/main`). Update every pin field (`pin.revision`, `pin.blob`, `pin.vectors.revision`, `pin.vectors.blob`, `pin.vectors.bytes`, and anything else the file pins), and let `schema_conformance`'s step 0 prove the copy is the artifact the pin names. The hub reports that its gate is green at 126 pass / 187 red / 77 closure after this change, and that **cases 5, 15 and 17 are today's replies and go red on the re-vendor**: establish from `tests/schema_conformance.rs` how our suite turns those into a failure (which rows judge LIVE replies against the vendored schema), show that failure once (re-vendor applied, serve not yet), then show it green with the serve. The serve and the re-vendor may be separate commits; say which intermediate commit, if any, is red.

## 4. Tests: CR1-CR9 from the CR's §7, plus the §8 rewrites

- CR1-CR5, CR6 (extend `tests/layers.rs::the_mask_vocabulary_is_the_contract_fragments_own` so `LayerMask::targets()` equals `$defs/displayMask.items.enum` in BOTH directions), CR7 (player parity: a mask set through `Engine::set_layer` gives a wire `displayMask` equal to `LayerMask::hidden()`), CR9 (`screenshot`, `scanlines` and `state_hash` agree at the same machine point, `[]` unmasked).
- **CR8, and M1 governs it.** With no mask and both sources `raster`, FNV-1a-64 over `scanlines`' active-display rows (as `scanlines` serves them, at the reply's `mode` width after its normalization), decoded and concatenated, equals `framebuffer`; under a mask that hides drawn content the same fold over the masked rows differs (the anti-vacuity half). **Run it on an H40 frame AND an H32 frame** (`tests/scanlines.rs` has an H32 fixture at ~`:346`). Derive the row range from `scanlines`' own bound (the constant or the served range), never a typed `224`. **If CR8 fails on either width, do NOT bend the test or the fold to make it pass**: report exactly what differs (widths, lengths, first differing byte offset). The hub withdraws R2 by a delta ruling in that case; that is a legitimate outcome of this parcel, not a failure of it. If H32 cannot be measured, say why and record it as unmeasured.
- §8's rewrites: `tests/methods.rs` (~`:223-227`, the two constant-caveat assertions) flips to "no caveat" and gains CR5; `tests/layers.rs` (~`:791-986`, `unmasked_state_hash_caveat` and the strict-extension assertions) is rewritten around the typed key. **Keep every "the hash does not move under a mask" assertion and the anti-vacuity precondition** (the mask must be shown to reach the picture before the hash's sameness means anything).
- The masked-read caveat tests at `tests/layers.rs` ~`:652-697` (screenshot/scanlines under a mask) are NOT removed by this ruling; keep them green, and say if the new key changes what they should assert.

## 5. Consuming surfaces (the claims this ruling makes false)

This parcel repairs claims as well as code. A claim repaired at its canonical site leaves its paraphrases standing. Sweep **by varying the spelling AND the axis**, not only the words the canonical site uses: "VDP state only", "covers VDP state", "always carries a caveat", "constant caveat", "full VDP", "its own reply says so", "M18 … still in the tree", and anything that restates the peek property or which picture `framebuffer` is. Known sites from the CR's §8: `masked_hash_caveat`'s doc (~`engine.rs:3699-3708`; its "no mask keeps the unmasked reply byte-identical" becomes the D1 rule), `tests/pixel_attribution.rs` ~`:732-738`, `tests/pacing.rs` ~`:657-660`, the peek doc at `engine.rs` ~`:4680`. Search `crates/**` (code, comments, doc strings, test messages) and the player (`crates/oracle-player`) for any consumer that shows or parses a `state_hash`/`read_vram` caveat. Dated `docs/2026-*.md` records are history: do not edit them. List every site you changed and every site you checked and left, with why.

## 6. Hard boundaries

- **Files in scope:** `crates/oracle-aether/src/engine.rs`; `crates/oracle-aether/tests/**` (including `tests/contract/` for the re-vendor); doc comments only in `crates/oracle-core/src/state_hash.rs`; `crates/oracle-player/**` only where a consumer of the changed replies is found (list each); `docs/lens-findings.jsonl` (ledger, below). Anything else: say why in the report before touching it.
- Do not touch `docs/OVERSEER.md`, `docs/lane-status.json`, the CR draft under `docs/proposed/`, or any sibling repo (empyrean, oracle-old: read-only).
- **Wire changes are exactly the ruling's**: the added keys and the removed constant caveats. Any other reply-shape or text change on the bus is out of scope; if one seems needed, BLOCKED-and-report that item.
- No emulation behaviour change. No hash value change.

## 7. Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run); (b) restore from a **committed** baseline, never `git checkout --` over uncommitted work; (c) actually red: applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"); (d) wired into `cargo test --workspace`, expectations **derived** from source/constants/the vendored schema, never from the implementation under test, loud on unmeasurable (never render "couldn't measure" as 0 or green); (e) if you tighten the method midway, re-establish earlier claims under it and say which were re-established and how. **For every assertion, ask: if this went green for a reason OTHER than the rule holding, what would that reason be?** Name it in the report and say how you ruled it out. Useful mutations here: emit `displayMask` unconditionally; emit it from a second hand-written list; apply the mask to `framebuffer`; drop it from `scanlines` only; fold CR8 at the wrong width.

## 8. Running and verifying

- **One cargo invocation at a time in your worktree.** Targeted runs (`cargo test -p oracle-aether --test <file>`) while you work are fine.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh; never copy it over itself. Do not commit while it runs.
- Baseline, **release profile**, from `tools/land.sh --no-push` at `f7c0cd9`: **89/89 legs, 2858 passed / 0 failed / 3 ignored**. **Predict your leg count and pass count before the run** and report prediction vs result; a moved leg count is an explanation or a problem, so say which.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message** (a death must cost the run, never the work); messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.
- Ledger: append ONE line each for lens **M16** and **M18** to `docs/lens-findings.jsonl`, copying the shape of that id's latest line (`id, sweep, seat, severity, title, batch`; `state: "fixed"`, `fixedAt` from `git rev-parse`, `at` from `date -u +%Y-%m-%dT%H:%M:%SZ`, `detail` naming §11.49). If an id has no line yet, enrol it in the shape of its neighbours and say so. Append only; validate it parses. Fix commit first, ledger commit after (a `fixedAt` cannot name its own commit).

## 9. Standing rules

- **Never use any emulator or MCP tool** (`mcp__oracle__*`); never launch a window on any display. TAG anything needing a live run.
- **BLOCKED is always available** per item: stop that one, record why, continue with the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## 10. Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. Per item (D1, D3, B/M4, R1, M2 verdict, re-vendor, CR1-CR9, CR8 on H40 and H32 with numbers): what changed, tests added by name, each mutation (line on disk → named failure → restore), the alternative green-path you ruled out. 4. The before/after fingerprints proving no hash moved. 5. Consuming surfaces changed and checked. 6. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored (predicted vs actual), profile. 7. Open items, new findings, TAGs, anything BLOCKED.
