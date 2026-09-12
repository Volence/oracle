# LENS-WAVE4 (CORE-TYPES + MEMORY-PAYLOAD + wave-3 residue): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 5).** Written from `docs/2026-09-12-lens-wave3-brief.md`'s shape.
> Premises re-measured at `fe11c02` before dispatch: `StateHash::compute(vram: &[u8], cram: &[u8], vsram: &[u8],
> regs: &[u8])` at `crates/oracle-core/src/state_hash.rs:44`, guarded by four `debug_assert_eq!` lengths; `port: usize`
> on every `Io` accessor (`io.rs:125-187`), where `set_pad`/`pad` `assert!(port < 2)` and `read_data` treats port 2 as a
> released pad, then through `System::set_pad`/`pad` (`system.rs:765`/`:770`), `Host::held` (`host.rs:354`),
> `Engine::held` (`engine.rs:2177`) and `engine.rs`'s `InputRow`/`pad_at` (`:10258`/`:10269`);
> `write_params`' `payload.trim().trim_start_matches("0x")` at `crates/oracle-player/src/memory.rs:414`; `held_names` at
> `engine.rs:10481`; the stale refusal sentence at `crates/oracle-player/src/screen_pick.rs:895`; the width rule
> `if h40 { 320 } else { 256 }` at `vdp.rs:82` and `render.rs` `:1726`, `:2173`, `:2234`, `:2366`. No sibling repo's
> `Cargo.toml` names `oracle-core` or `oracle-aether` (grepped aeon, sigil, aurora, seraph, empyrean, dominion,
> oracle-old; control: the same grep finds three in oracle), so the public-API changes have no consumer outside this
> workspace. Baseline `tools/land.sh --no-push` at `0f0b12d` (`fe11c02` is docs-only on top): 89/89 legs, release
> 2829 passed / 0 failed / 3 ignored. STATE-HASH-TYPED-MASK is NOT in this wave (M16 needs a CR first), nor M50's
> `memory.rs` half (its look call is unanswered). The text below the rule is the agent prompt, passed with
> `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator + its Aether debug bus). Parcel: **LENS-WAVE4**, two wave-4 parcels of the triage plan of record, `docs/2026-09-11-lens-triage.md` ("Wave 4 and later" and the "Updated after wave 3" addendum above it), plus the wave-3 residue that addendum books. Size M. One branch, one commit set per row or tight group.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it. The triage judged these rows from one-paragraph packet summaries (`docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md`, entries M70, M76, M77), so **re-verify each row holds at HEAD before you fix it**. A row that no longer holds gets a ledger line (`state: "fixed"` with the commit that fixed it, or `"refuted"`) and no code. Earlier agents on this lane found the booked counts wrong in both directions (the last one reported three width-rule copies; the merged tree had five). Derive every population by varying the spelling AND the axis of your search (a literal, a derived expression, a comment that restates the fact, a test copy), and enumerate a type's sites by what TOUCHES it (every constructor, caller and copier), not only by its name.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor fe11c02 HEAD && echo OK` must print OK and `docs/2026-09-12-lens-wave4-brief.md` must exist; otherwise BLOCKED, stop.
- `git switch -c parcel/lens-wave4`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/lens-wave4-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## 1. Parcel A: CORE-TYPES (M70, M76)

**M70.** `StateHash::compute` takes four interchangeable `&[u8]` guarded only by debug-build length checks. The packet's point: fixed-size array types (`&[u8; VRAM_SIZE]` and friends) would make a wrong length **and an argument-order swap** compile errors, and they happen to have four different sizes, so a swap cannot type-check. Make the types carry it. Find every caller (at least `system.rs:849`, the two in `vdp.rs:4120`/`:4121`, the two tests in `state_hash.rs`) and decide how each supplies an array (the accessors returning arrays, or a checked conversion at the one boundary); say which and why. If a `debug_assert_eq!` becomes redundant, delete it and say so; do not leave a check that can no longer fire. Red-first here means showing that the swap and the wrong length now fail to COMPILE: put a `compile_fail` doctest (or the equivalent) on the signature and show it passing because compilation fails, plus a control that the correct call compiles.

**M76.** `port: usize` carries a two-valued rule ("ports 0 and 1 have a pad; port 2, EXP, has a Data/Ctrl/serial register but no pad") through four layers (`Io` → `System` → `Host`/`Engine` → the wire), and the layers disagree: `Io::read_data(2)` answers gracefully while `Io::pad(2)` panics, and `read_data(3)` indexes out of bounds. The house style documents panicking accessors elsewhere; this one is not documented as one. Make the rule a type (e.g. a `PadPort` for the pad half and a `Port` for the register half; your design, justified) so that "a pad on EXP" and "port 3" are unrepresentable below the wire, and the one place an integer becomes a port is the wire boundary, where the existing refusal already lives. **No wire change**: the `port` param stays a number, and every existing reply and refusal (text included) is byte-identical; show that with the existing wire tests plus a new one for port 2 and port 3 if none covers them. The bus-side register decode (whatever maps `$A10003`… to a port index) is a consumer too; find it by what touches `Io`, not by the word `port`.

**Consuming surfaces, not only the code.** These rows repair CLAIMS as well as code. Doc comments that restate "four byte regions", "ports 0/1 only", "EXP has no pad" or the panic must end true. A claim repaired at its canonical site leaves its paraphrases standing, and the paraphrases are where the next parcel falsifies it. List what you changed.

**Currency is untouched:** zero diff under `crates/oracle-core/tests/` golden files; `state_hash` and `export_state` outputs byte-identical (the FNV layout and byte order are a suite contract; see the doc on `StateHash`).

## 2. Parcel B: MEMORY-PAYLOAD (M77)

`crates/oracle-player/src/memory.rs` `write_params` cleans a typed payload with `trim_start_matches("0x")`, which strips **repeatedly**: `0x0x41` becomes `41` and is sent as a byte the server's own parser (`oracle_aether::hex`, which the comment at `:429` names) would refuse, while `0X41` and `$41`, which that parser accepts, are refused by the panel. The fix is for the panel to accept exactly what the blessed parser accepts and refuse the rest, by using it or by sharing its rule, never by writing a third spelling. Establish from source what `hex::parse_bytes` (or its current name) accepts, and derive the panel's tests from that, not from this paragraph. The refusal text is read by a person: keep it telling them what to type. Also check the other `trim_start_matches("0x")` sites in `memory.rs` (≈`:577`, `:588`, `:893`, `:985`): the ones that parse a **server reply** (always `0x`-prefixed by contract) are a different case from ones that parse **human input**; say which each is and fix only the human-input ones, or say why none need it.

**NOT in scope: M50** (the Memory box and the breakpoint box disagreeing about a bare `1000`). Its fix waits on a look call the owner has not made. If your M77 change would decide M50's question in passing, stop at the line and report it.

## 3. The wave-3 residue (booked in the triage addendum "Updated after wave 3")

1. `held_names`' inline button table in `engine.rs` (≈`:10481`) is a second copy of `BUTTONS_3`. Make it use the owner. Red-first the way wave 3 did M69: a mutation to `BUTTONS_3` must now redden a named test where it would have degraded silently.
2. The Screen tab's refusal in `screen_pick.rs` (≈`:895`) still says "the masked re-render is failing", which `render_masked` can no longer do (M42, wave 3). Make the sentence true to what CAN now cause that refusal (read the refusal's condition and say what reaches it). It is text a person reads, so the whole rendered sentence goes in the test, not a fragment.
3. `testrom.rs` `build_cram_midframe`'s two unnamed `0xE0` arguments back the scanline goldens: name them (or derive them from `vdp::ACTIVE_LINES` if that is what they mean; establish which from the ROM builder, not from the number). Goldens byte-identical.
4. The width rule `if h40 { 320 } else { 256 }` still has FIVE copies beside its owner `Vdp::active_display()` (`render.rs`): `render.rs` `resolve_line_masked`, `pixel_attribution_masked` and `line_report_from` (all `&self`), `advance_scanline` (`&mut self`), and `vdp.rs:82`. Fold them into one owner. **The masked-render invariant from `docs/2026-08-26-layer-mask.md` binds you**: no render taking a `LayerMask` may take `&mut self`, and `render_scanline` does not gain a mask parameter; a shared width function over `h40` (or over `&self`) is safe, a change to any `&mut self` render's behaviour is not. The parity rows wave 3 added (`Vdp::render_frame_masked` vs its consumers) must stay green; show one width mutation in the new owner reddening them by name.
5. `PROF_VBLANK_LINE`'s compile-time assertion (`testrom.rs`, tied to `ACTIVE_LINES` in wave 3) was never mutation-tested. Mutate it on disk, show the build failing with the assertion's message, restore.

## 4. Hard boundaries

- **Files in scope:** `crates/oracle-core/src/{state_hash.rs,io.rs,system.rs,vdp.rs,render.rs,testrom.rs}` and whatever `oracle-core` file decodes the I/O port addresses; `crates/oracle-aether/src/{engine.rs,host.rs}` and its tests; `crates/oracle-player/src/{memory.rs,screen_pick.rs,ui.rs}`; `crates/oracle-core/examples/` and any other workspace caller the type changes force (list each); new tests anywhere sensible. Anything else: say why in the report before touching it.
- Do not touch `docs/OVERSEER.md` or `docs/lane-status.json` (the overseer's files; the second is parsed by a console outside this repo). Do not touch the vendored contract/schema.
- **No wire change**: no new result key, no changed reply shape or text on the bus. If a row turns out to need one, BLOCKED-and-report that row.
- No emulation behaviour change anywhere: every change here is a type, a name or a text.

## 5. Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run); (b) restore from a **committed** baseline, never `git checkout --` over uncommitted work; (c) actually red: applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"); (d) wired into `cargo test --workspace`, expectations **derived** from source/constants, never from the implementation under test, loud on unmeasurable (never render "couldn't measure" as 0 or green); (e) if you tighten the method midway, re-establish earlier claims under it and say which were re-established and how. **For every assertion, ask: if this went green for a reason OTHER than the rule holding, what would that reason be?** Name it in the report and say how you ruled it out.

## 6. Running and verifying

- **One cargo invocation at a time in this repo.** Targeted runs (`cargo test -p <crate> --test <file>` / `--lib`) while you work are fine. A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh; never copy it over itself. Do not commit while it runs.
- Baseline, **release profile**, from `tools/land.sh --no-push` at `0f0b12d`: **89/89 legs, 2829 passed / 0 failed / 3 ignored**. **Predict your leg count and pass count before the run** and report prediction vs result; a moved leg count (a new test target) is an explanation or a problem, so say which. Doctests count: a `compile_fail` doctest adds to the doc-test leg's total.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each row lands, **findings in the message** (a death must cost the run, never the work); messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.
- Ledger: for each lens row you fix (M70, M76, M77), append ONE line to `docs/lens-findings.jsonl` copying that row's shape (`id, sweep, seat, severity, title, batch` from its latest line; `state: "fixed"`, `fixedAt` from `git rev-parse`, `at` from `date -u +%Y-%m-%dT%H:%M:%SZ`, `detail`). A row left partly open gets `state: "open"` with the residue named. The wave-3 residue items are not ledger rows; report them in section 5 of your report. Append only; validate it parses. Fix commit first, ledger commit after (a `fixedAt` cannot name its own commit).

## 7. Standing rules

- **Never use any emulator or MCP tool** (`mcp__oracle__*`); never launch a window on any display. TAG anything needing a live run.
- **BLOCKED is always available** per row: stop that one, record why, continue with the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## 8. Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. Per row (M70, M76, M77, residue 1-5): verdict at HEAD, what changed, tests added by name, each mutation (line on disk → named failure → restore), the alternative green-path you ruled out, consuming surfaces changed. 4. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored (predicted vs actual), profile. 5. Open items, residue, new findings, TAGs, anything BLOCKED.
