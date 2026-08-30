# The live-tree readers the freeze did not reach — closed, and one class left deliberately open

**Branch** `parcel/live-tree-readers`, off `main` at `e876f10a`.
**Scope** every site in this repo that reads Aeon's *live working tree* at
`/home/volence/sonic_hacks/aeon` rather than our own frozen `fixtures/aeon/`, plus the stale
expectations pinned against whatever that tree happened to hold.

| commit | what |
|---|---|
| `254ab9e` | the four examples: `common/rom_source.rs` (new), `vgm_capture.rs`, `diag_soundqueue.rs`, `synth_render.rs`, `k4_openbus_probe.rs` |
| `7bb7331` | `tools/aether_smoke.py` — four stale pins derived at run time, the frozen cross, and two screenshot checks that had gone red and vacuous |
| `ee72d16` | this report |
| `2f5d99b` | `rustfmt` on `rom_source.rs`, and `k4_openbus_probe`'s last two raw path literals routed through `LIVE_AEON_DIR` (see §5) |

`fixtures/aeon/` ended that dependency for the **tests**. It did not reach the **tools**: five files
still resolved an absolute path into another lane's working directory, and one of them pinned four
numbers to a build of it from 2026-08-14. All four numbers were wrong by the time this parcel opened.

---

## 1. The complete enumeration, with the commands that produced it

A single grep is one question, and no one alphabet is a superset of another: a quoted-path search
misses `aeon_dir()`, an identifier search misses the string literal, and neither finds a hardcoded
symbol count. Five were run, from the repo root.

```sh
# A — the live path literal
grep -rn "sonic_hacks/aeon" . --exclude-dir=.git --exclude-dir=target

# B — the resolver identifiers
grep -rn "AEON_DIR\|aeon_dir\|AeonDir" --exclude-dir=.git --exclude-dir=target .

# C — the frozen directory's own name
grep -rn "fixtures/aeon\|fixtures\\\\aeon" --exclude-dir=.git --exclude-dir=target .

# D — the artifact basenames, in code and scripts only (docs data files excluded: two of them are
#     append-only ledgers whose hits are all narrative)
grep -rn -E "s4\.bin|s4\.debug\.bin|s4\.soundtest\.bin|demo\.bin|demo\.debug\.bin|s4\.lst|\
s4\.debug\.lst|demo\.lst|demo\.debug\.lst" \
  --include='*.rs' --include='*.py' --include='*.sh' --include='*.toml' --include='*.json' \
  --include='*.yml' --include='*.yaml' . | grep -v '^./docs/' | grep -v '^./target/'

# E — relative references, which A cannot see
grep -rn -E "\.\./aeon|[^-a-z]aeon/" --include='*.rs' --include='*.py' --include='*.sh' \
  --include='*.toml' . | grep -v '^./target/' | grep -v 'fixtures/aeon' | grep -v 'sonic_hacks/aeon'

# F — hardcoded counts of the kind that go stale on a pin move
grep -rn -E "symbolCount|symbol_count|2129|2743" --include='*.rs' --include='*.py' --include='*.sh' . \
  | grep -v '^./target/'
```

Every command exited 0 with output; none of the emptiness-as-finding traps applies, and no
`2>/dev/null` was used anywhere in the sweep.

### What they found — every executable site that reads the live tree

| # | site | artifact | frozen copy exists? | disposition |
|---|---|---|---|---|
| 1 | `tools/aether_smoke.py:5` (documented launch line) | `../aeon/s4.bin` | **yes** | repointed to `fixtures/aeon/s4.bin` |
| 2 | `tools/aether_smoke.py:140` | `/home/…/aeon/s4.debug.lst` | **yes** | repointed to the frozen copy, resolved from the script's own location |
| 3 | `tools/aether_smoke.py:84` | — | — | stale pin `symbolCount == 2129` → derived at run time (§3) |
| 4 | `tools/aether_smoke.py:85` | — | — | stale pin `romBytes == 696836` → derived at run time (§3) |
| 5 | `tools/aether_smoke.py:89-90` | — | — | stale pins `Player_1 = 0x00FF8CFA / 0xFFFF8CFA` → derived at run time (§3) |
| 6 | `crates/oracle-core/examples/vgm_capture.rs:22` | `s4.bin` | **yes** | default repointed to `fixtures/aeon/s4.bin` |
| 7 | `crates/oracle-core/examples/diag_soundqueue.rs:29` | `s4.soundtest.bin` | **no** | live default KEPT, made loud (§2) |
| 8 | `crates/oracle-core/examples/synth_render.rs:17` | `s4.soundtest.bin` | **no** | live default KEPT, made loud (§2) |
| 9 | `crates/oracle-core/examples/k4_openbus_probe.rs:320` | `s4.bin` | **yes** | row repointed to `fixtures/aeon/s4.bin` |
| 10 | `crates/oracle-core/examples/k4_openbus_probe.rs:321` | `s4.soundtest.bin` | **no** | live path KEPT, row marked (§2) |
| 11 | `crates/oracle-core/examples/k4_openbus_probe.rs:322` | `demo.bin` | **no, but obtainable** | live path KEPT, row marked; freeze declined with reasons (§2) |

The controller's starting list named 1, 2, 3, 6, 7, 8, 9, 10, 11. **Rows 4 and 5 are new** — two more
literals in the same three-line block of `aether_smoke.py`, pinned to the same 2026-08-14 build, both
of them already wrong. They were found by alphabet F, not by the path alphabet that found the others.

### Sites the sweep found and this parcel deliberately did NOT change

* **`tools/blastem-differential/{build_rom,build_vdp_pending,build_vdp_dma_fill}.sh:6-9`** —
  `TOOLS="${TOOLS:-$HERE/../../../aeon/tools}"`. This resolves aeon's *assembler tools*, not their
  build artifacts. It is a toolchain dependency, already `TOOLS=`-overridable, and the ROMs it builds
  come from `.asm` sources committed here. Different class; out of scope for a data-staleness parcel.
* **`docs/OVERSEER.md:1184-1185`** — states that `replay_real_artifacts.rs` defaults to
  `/home/volence/sonic_hacks/aeon`. That sentence sits inside the **`F-REPLAY-READS-AEONS-BUILD`
  finding block, which is explicitly marked CLOSED directly above it and "kept in full"**. It is a
  record of what was true when the finding was registered, not a current-state claim, and this repo's
  supersession rule says such text stays readable next to its reversal.
* **`crates/oracle-core/examples/vgm_capture.rs`'s `OUT_VGM` scratchpad constants** still name
  `-home-volence-sonic-hacks-oracle-next`, a directory that predates the repo rename. Stale, but a
  different defect (an output path, not a live-tree read). Left alone; noted here so it is not lost.
* **`crates/oracle-core/tests/symbols_real_lst.rs:28` and `crates/oracle-replay/tests/replay_real_artifacts.rs:72`**
  mention the live path inside a doc comment explaining what `ORACLE_AEON_DIR` overrides *to*. Prose,
  not a read.

### Presence is not behaviour: what actually runs

```sh
grep -n "example\|aether_smoke\|\.py\|\.sh" .github/workflows/ci.yml
#   43:  run: ./tools/fetch-tests.sh
#   78:  run: ./tools/replay_playthroughs.sh
```

**None of the five files is invoked by CI or by any script in the repo.** A search for each name
across `*.rs *.py *.sh *.toml *.yml *.json` returns only the files' own definitions, one prose mention
of `k4_openbus_probe` in `watchpoints.rs`, one of `synth_render` in `audio_sink.rs`, and
`synth_render`'s `[[example]]` stanza in `Cargo.toml`. They are **hand-run developer tools**.

That matters for how hard each needed fixing, and it cuts both ways. Nothing was gating on them, so
none of this was blocking a green — but it also means **nothing was ever going to tell anyone they had
gone stale**, which is exactly how four literals in `aether_smoke.py` stayed wrong long enough for all
four to be wrong at once. `cargo test` does *compile* every example, so the paths had to keep
compiling; they never had to keep being *true*.

---

## 2. The rule applied to unfrozen artifacts, and why it is not uniform

`git ls-files fixtures/aeon/` — verified, not taken from the brief:

```
fixtures/aeon/PIN.tsv          fixtures/aeon/demo.lst        fixtures/aeon/s4.debug.bin
fixtures/aeon/PROVENANCE.md    fixtures/aeon/demo.debug.lst  fixtures/aeon/s4.debug.lst
fixtures/aeon/s4.bin           fixtures/aeon/s4.lst
```

Six artifacts. `s4.soundtest.bin` and `demo.bin` are not among them.

**The rule:**

> **If a frozen copy exists, the default points at it.** Deterministic, committed, always present,
> attributable to a chain.
>
> **If it does not, the live-tree default stays and the dependency is made LOUD** — the program states
> at startup which file it read, that it is not frozen, and how old that file is, so a stale read
> announces itself instead of passing as a measurement.

**Why "loud" and not "take a path argument with no default".** These are hand-run diagnostics whose
entire ergonomic value is `cargo run --example diag_soundqueue` with nothing after it. Removing the
default converts a one-liner into a path hunt on every invocation, for every user, forever. The
failure mode actually being guarded against is a *silent* stale read, and an announcement closes that
completely: the operator sees the tree and the file's age before a single number is printed. The
argument still overrides, as it always did.

**Why `s4.soundtest.bin` is not frozen — BLOCKED, not skipped.** `fixtures/aeon/PROVENANCE.md` makes
sigil's committed goldens the authority for ROM bytes. Checked at the pinned freeze, firsthand:

```sh
git -C /home/volence/sonic_hacks/sigil ls-tree -r --name-only 39c34fd2:crates/sigil-harness/golden/
#   … config_a.bin  config_b.bin  demo.bin  demo.debug.bin  lean.bin  s4.bin  s4.debug.bin …
```

`s4.soundtest.bin` is absent. There is nothing chain-attested to freeze it from, and the brief's rule
is explicit that this is a BLOCKED note rather than a hunt. Recorded as such.

**Why `demo.bin` is not frozen, which is a JUDGEMENT and not a blocker — say so plainly.** That same
listing shows `demo.bin` **is** in sigil's goldens at `39c34fd2`, so unlike `s4.soundtest.bin` it
*could* be frozen. It was not, and the reasoning is:

* Its only consumer is **one row of a skip-if-absent survey table in a hand-run example**. The other
  ten rows of that table are commercial ROM dumps on the user's disk — the table's whole design is
  "probe whatever is here".
* Freezing it is not free: `fixtures/aeon/` is a **record**, not a cache. A seventh artifact means a
  `PIN.tsv` row, a `PROVENANCE.md` table row and sha256, and a paragraph on where it came from —
  and `aeon_pin.rs` asserts `PIN.tsv` lists *exactly* the directory's contents, so the three move
  together or the suite reddens.
* Freezing one aeon row of eleven makes the corpus *half* deterministic, which reads as more
  authoritative than it is.

So it stays live and the row says so. **This is a mixed outcome and it is described as one:** rows 7,
8, 10 and 11 still read a tree we do not control. What changed is that they can no longer do it
quietly.

### What "loud" looks like, run firsthand

Frozen (`vgm_capture`, no arguments):

```
ROM /home/…/fixtures/aeon/s4.bin: 719315 bytes
  FROZEN — this repo's own committed copy (fixtures/aeon/PROVENANCE.md)
```

Unfrozen (`diag_soundqueue`, no arguments):

```
ROM /home/volence/sonic_hacks/aeon/s4.soundtest.bin: 429321 bytes
  ⚠ NOT FROZEN — outside fixtures/aeon/, so these bytes are whatever was on disk when
    this run read them (last modified 919 h 58 min ago). If the path is in Aeon's working
    tree, that tree is rebuilt without warning and this run is not reproducible from
    the repository alone.
```

That age is not decoration. `s4.soundtest.bin` on this disk was last written **2026-07-22**, five and
a half weeks before every note that cites it. The size alone could not have said so — chain 188's and
chain 189's `s4.debug.bin` are both exactly 736,315 bytes, which is precisely the trap
`PROVENANCE.md` warns about under *"Compare hashes, never lengths"*. A hash would say it better still;
`oracle-core` carries one dependency on purpose and a dev example is not where a second gets added.

`k4_openbus_probe`, run firsthand — the marker rides the row, not a comment:

```
| aeon-s4 (900/900f)                                        | … |
| aeon-s4-soundtest (900/900f) [LIVE aeon tree, NOT frozen] | … |
| aeon-demo (900/900f) [LIVE aeon tree, NOT frozen]         | … |
```

The rule lives once, in `crates/oracle-core/examples/common/rom_source.rs`, rather than in four copies
of the same paragraph. Cargo's example discovery finds `examples/*.rs` and `examples/*/main.rs`, so a
plain file in a subdirectory is not itself built as an example; each consumer includes it with
`#[path = "common/rom_source.rs"] mod rom_source;`.

---

## 3. `aether_smoke.py:84` — the derivation

### Which listing the check actually loads

Established before deriving anything, because the file names differ. The server auto-binds the
listing sitting beside the ROM, and reports it: with the smoke's launch line naming `s4.bin`, the run
prints

```
       romPath    /home/…/fixtures/aeon/s4.bin
       symbolsPath fixtures/aeon/s4.lst
```

So the check loads **`s4.lst`**, the release listing — consistent with its own label and with line 80
(`symbolsLoaded true (s4.lst bound)`). Not `s4.debug.lst`.

### The derivation

`symbolCount` is defined at `crates/oracle-aether/src/engine.rs:2353` as
`self.symbols.as_ref().map_or(0, |t| t.len())` — the number of rows that carry an address, which can
legitimately be *below* a listing's own `N symbols` footer when the assembler emits addressless build
metadata (`engine.rs:4980-5011`).

The frozen `fixtures/aeon/s4.lst` declares its own count at line 4625:

```
$ grep -n -E "^\s*[0-9]+ (symbols|unused symbols|equates)" fixtures/aeon/s4.lst
4625:   2310 symbols
4626:    0 unused symbols
5355:   723 equates
```

And this repo's own parser, run over exactly those bytes, produces that same number and asserts the
equality rather than reporting it:

```
$ cargo test -p oracle-core --test symbols_real_lst -- --nocapture
  read from …/fixtures/aeon
  s4.lst: 2310 symbols, 57 modules
  s4.lst: Player_1 listed as $FFFF8E48 = bus $FF8E48
  test real_s4_lst_parses_completely ... ok      # asserts matches_declared_count() == Some(true)
  test result: ok. 10 passed; 0 failed
```

**So the correct expectation is 2310, not 2129.** It is derived twice over, from two independent
implementations (a footer regex here; the Rust `SymbolTable` there), and corroborated a third time by
`PROVENANCE.md`'s recorded `replay_runner` output — *"lst: 2743 symbols"* for `s4.debug.lst`, whose
footer likewise declares 2743.

### But the fix is not 2310

A literal that must be hand-updated on every pin move is a maintenance trap, and the trap has a
specific shape: **the only way anyone ever updates such a literal is by copying whatever the run just
printed** — which converts a gate into a transcription of the current behaviour, a check that cannot
fail. The pin has already moved twice this week.

So the check now reads the footer out of the listing the server *says* it loaded:

```python
feet = re.findall(r"(?m)^\s*(\d+)\s+symbols\s*$", lst_text)
…
check(f"symbolCount == {want_syms} (the listing's own footer)",
      st_["symbolCount"] == want_syms, st_["symbolCount"])
```

This is **stronger than the literal, not weaker**, on three counts. It survives every pin move without
a human. It additionally asserts that the server ingested *every* declared row — a silent shortfall
now reddens, where `== 2129` would have been indifferent to it. And it makes the script correct
against any ROM, which is what a protocol smoke test is for.

Equality rather than `<=`, deliberately: sigil's listings emit no addressless rows (the equality above
is asserted for exactly these bytes), so a shortfall would mean the emitter changed — worth reddening
on rather than absorbing. The reasoning is in the code beside the assertion.

`romBytes` and the two `Player_1` values were fixed the same way: the image on disk is the authority
for its own length, and the listing is the authority for a symbol's raw spelling. The **24-bit bus
mask is applied in the script independently**, from the rule rather than from the file, because that
mapping is the property under test.

### Red-first, against the live server

The old literals, run against the same running server the new checks pass against:

```
OLD LITERAL ASSERTIONS, run live against the frozen ROM:
  FAIL  symbolCount == 2129              -> server reports 2310
  FAIL  romBytes == 696836               -> server reports 719315
  FAIL  Player_1 addr == 0x00FF8CFA      -> server reports 0x00FF8E48
  FAIL  Player_1 rawAddr == 0xFFFF8CFA   -> server reports 0xFFFF8E48
```

Four for four. Not one of the pins was still true.

---

## 4. Found, not briefed: two more checks in the same file, one of them vacuous

The smoke script was **already red before this parcel**, and the enumeration surfaced why.
`emulator/screenshot` was changed from PPM to PNG (`engine.rs:4584-4590`, a deliberate change with its
reasoning in place). Two checks were never moved across:

* `check("screenshot is a full frame", size == P6-header + w*h*3, …)` — **FAILING** against every PNG
  since the change. Measured live: expected 215,055 bytes, file is 38,321.
* `check("frame is not blank", len(set(px[20:])) > 4, …)` — **VACUOUS**. It counted distinct bytes in
  a *compressed* stream, which is ~256 for any picture whatsoever.

That second one is the more serious of the two: a check that cannot fail, sitting in a file whose
other four expectations had silently gone stale. Both are now derived from the format the server says
it wrote — PNG magic and `format`, the reported byte count against the file, the IHDR against the
reply's own `width`/`height`, then the IDAT stream decompressed so the raster length is checked as
`h * (1 + w*3)` and blankness is judged on **pixels**.

Red-first, on a synthetic all-black 320×224 frame that both forms must judge:

```
BLANKNESS CHECK, on a synthetic ALL-BLACK frame (it MUST go red):
  PASS  OLD form: distinct bytes of the COMPRESSED file = 38   <- vacuous, a blank frame passes
  FAIL  NEW form: distinct bytes of the DECODED pixels  = 1    <- correctly red
```

`zlib` and `struct` are standard library; the script's "shares no library with anything" rule is about
not sharing code with the server, and this shares none.

One real bug was introduced and caught during this: the PNG dimensions were first unpacked into `w, h`,
shadowing the `h` that holds the earlier `state_hash` reply and breaking a comparison forty lines
later with a `TypeError`. Renamed to `iw, ih`, with the reason recorded at the line.

---

## 5. Verification

| what | result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy -p oracle-core --all-targets -- -D warnings` | clean |
| `cargo clippy -p oracle-core --all-targets --features synth -- -D warnings` | clean |
| `cargo build --release -p oracle-core --examples` (± `--features synth`) | clean |
| `cargo test -p oracle-core --test symbols_real_lst -- --nocapture` | 10 passed, 0 failed (3 s) |
| `cargo test --workspace` | **62 legs · 1992 passed · 0 failed · 6 ignored · 0 SKIP notes · exit 0**, 10 min 36 s wall (11:09:29 → 11:20:05 UTC) |
| `tools/aether_smoke.py` against a live `oracle-aether` on `fixtures/aeon/s4.bin` | **22 checks, 0 failures, exit 0** — was exit 1 before |

No failing test names to list: the failure grep over the run's full output returns nothing
(`grep -n "FAILED\|panicked\|^error\[" … ; exit 1`), and the 62 `test result:` lines sum to
1992/0/6 with all 62 reading `ok`.

All four examples were **run**, not merely compiled: `vgm_capture` (frozen banner, 600 frames),
`diag_soundqueue` and `synth_render` (unfrozen banner with age), `k4_openbus_probe` (row markers on
the two live rows, none on `aeon-s4`).

**A miss worth writing down, because it is the shape this repo keeps catching.** `254ab9e` was
committed **not `rustfmt`-clean**. The last `cargo fmt --all -- --check` before it ran *before* the
final rewrite of `announce`'s output block, and the pass was carried forward as though it still
described the tree. It does not: a green is a statement about the bytes that were checked, not about
the file. Fixed in `2f5d99b`, which also routes `k4_openbus_probe`'s last two raw live-tree literals
through `rom_source::live_aeon` so `LIVE_AEON_DIR` is the single place the tree is named and the row
marker cannot drift from the paths it judges. A detail that will help the next reader: `cargo fmt`
prints `rom_source.rs`'s diff **four times**, once per example target that includes it.

**A worktree ops note that cost a full suite run.** The first `cargo test --workspace` came back with
8 failures in `oracle-frontend`'s `save_state` — all of them
`vendored test ROM …/vendor/TestRoms/m68k_memory_test.bin is missing`. `vendor/` is gitignored, so a
fresh worktree has none; the fix is `ln -s <main checkout>/vendor <worktree>/vendor`. Nothing to do
with this parcel. Worth noting that these rows now **hard-fail with a pointer** instead of skipping
silently, which is the bar working.

---

## 6. Open, BLOCKED, and TAGGED

* **BLOCKED — `s4.soundtest.bin` cannot be frozen** on the current recipe: absent from sigil's
  chain-attested goldens at `39c34fd2` (verified above). Two examples default to it from a live tree
  and now say so. If it is ever wanted frozen, someone must first decide what attests it.
* **OPEN (a judgement, reversible) — `demo.bin` is freezable and was not frozen.** Reasoning in §2. If
  the seventh artifact is wanted, the recipe is `PROVENANCE.md`'s *"Moving the pin"* section plus a
  `PIN.tsv` row; `aeon_pin.rs` will refuse until the two agree.
* **OPEN — `vgm_capture.rs`'s output constants** still name the pre-rename scratchpad directory
  `-home-volence-sonic-hacks-oracle-next`. Cosmetic, unrelated class, untouched.
* **OBSERVED, not fixed — `emulator/status` is inconsistent about path shape.** It reports `romPath`
  absolute but `symbolsPath` exactly as it resolved it, which for the auto-bound sibling listing is
  *relative to the server's cwd*: this run printed
  `romPath /home/…/fixtures/aeon/s4.bin` next to `symbolsPath fixtures/aeon/s4.lst`. Anything
  resolving `symbolsPath` from a different working directory gets a path that does not exist. The
  smoke script handles it by failing loudly with the path it could not open, and says so at the line;
  making the server consistent is a contract question and not this parcel's to answer.
* **TAGGED for foreground — nothing.** Every claim in this document was measured in this worktree.
  The Aether server and the four examples are ordinary local binaries, not the emulator MCP surface,
  which was not touched.
