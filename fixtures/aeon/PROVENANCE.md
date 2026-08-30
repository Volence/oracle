# `fixtures/aeon/` — our frozen copy of Aeon's build artifacts

These six files are **this repo's own committed copy** of the Aeon build outputs our end-to-end tests
read. They are checked in as bytes — not a fetch script, not a checksum manifest. A fetch would
reintroduce exactly the dependency this freeze removes.

## Why this directory exists

Two test files used to read Aeon's *live* build outputs out of a sibling checkout at
`/home/volence/sonic_hacks/aeon`:

| test file | reads |
|---|---|
| `crates/oracle-replay/tests/replay_real_artifacts.rs` | `s4.debug.bin`, `s4.bin`, `s4.debug.lst`, `s4.lst` |
| `crates/oracle-core/tests/symbols_real_lst.rs` | `s4.lst`, `s4.debug.lst`, `s4.bin`, `s4.debug.bin`, `demo.lst`, `demo.debug.lst` |

That made our suite's green depend on another team's lane not rebuilding their game. On 2026-08-29 at
22:35 Aeon rebuilt and four rows in the replay file went red — **foreign failures, not our bug**. The
replay fixture the runner replays is *embedded inside* Aeon's ROM, so an Aeon rebuild moves the recorded
stream out from under us, with nothing on our side to point at.

Freezing our own copy means the pin moves only when *we* decide, and every move is attributable.
Hub ruling: empyrean `27b58fc` — *"oracle freezes its own ROM copy, aeon_rev attribution only"*.

`ORACLE_AEON_DIR` still overrides the directory, so a developer can deliberately point the tests at a live
Aeon build. What changed is only the **default**: it is now this directory rather than Aeon's working tree.

## What is pinned

**sigil freeze `5af70797` — `refreeze: fg-left-edge-borrow`, chain 186**, `aeon_rev def98ee5`.

### The two ROMs — from sigil's committed golden blobs

Taken as git blobs from sigil, never from a working tree (working trees move; sigil's goldens are frozen
and chain-attested):

```sh
git -C ../sigil show 5af70797:crates/sigil-harness/golden/s4.debug.bin
git -C ../sigil show 5af70797:crates/sigil-harness/golden/s4.bin
```

### The four listings — from an Aeon build tree

The `.lst` listings are **not frozen upstream**. sigil's golden set contains the ROMs (`s4.bin`,
`s4.debug.bin`, `demo.bin`, `demo.debug.bin`, `config_a.bin`, `config_b.bin`, `lean.bin`) and **no
listings at all** — verify with:

```sh
git -C ../sigil ls-tree -r --name-only 5af70797:crates/sigil-harness/golden/ | grep -i lst   # returns nothing
```

So an Aeon build tree is the only source that exists. These four came from
`/home/volence/sonic_hacks/.aeon-ref-186`, the Aeon worktree that produced chain 186: at capture it was
**clean** (`git status --porcelain` empty) and checked out at exactly chain 186's
`aeon_rev def98ee5d6fad568b022780caa6060cd70aa39e6`.

```
.aeon-ref-186/s4.lst          (mtime 2026-08-29 12:06)
.aeon-ref-186/s4.debug.lst    (mtime 2026-08-29 12:07)
.aeon-ref-186/demo.lst        (mtime 2026-08-29 12:09)
.aeon-ref-186/demo.debug.lst  (mtime 2026-08-29 12:10)
```

Captured **2026-08-30T03:55:58Z**.

### The consistency joint, and how it was checked

A listing is only valid for a ROM built from the same source. The ROMs come from sigil and the listings
from an Aeon tree, so that pairing has to be *proved*, not assumed. It was, at the moment of capture:
immediately after copying all six files, `.aeon-ref-186`'s on-disk ROMs were re-hashed and found
**byte-identical to sigil's `5af70797` golden blobs**:

```
75e9f4d4…1fcf7a  .aeon-ref-186/s4.debug.bin  ==  golden 5af70797 s4.debug.bin
b0873bed…be3351  .aeon-ref-186/s4.bin        ==  golden 5af70797 s4.bin
```

So the ROMs we froze and the listings we froze describe the same build. The suite re-proves the joint on
every run: `real_shape_binding_accepts_the_matching_rom_and_refuses_the_crosses` binds each listing to its
ROM through the `deb2` appendix and refuses both crosses, and `the_wrong_shape_listing_is_refused` checks
the release/debug cross from the replay side.

The two `demo` listings have no ROM counterpart here — the only test that reads them
(`real_demo_pair_documents_the_binding_checks_residual_limit`) compares the two listings against *each
other* and loads no demo ROM. They are frozen from the same build run.

## Build identity — `aeon_rev def98ee5`, attribution only

```
aeon_rev = "def98ee5d6fad568b022780caa6060cd70aa39e6"
```

Read out of sigil's `crates/sigil-harness/golden/provenance.toml` at `5af70797`, tip chain entry
(chain 186, `fg-left-edge-borrow`). In Aeon that commit is
`def98ee5 2026-08-29 11:20:48 -0400 "land(d-41): the left-edge fix the owner ruled in after seeing it run, with its design rule"`.

**This is attribution, not a dependency.** Nothing in this repo reads `aeon_rev`, resolves it, or moves
because it changed. It is recorded so a reader of these bytes can say which build they are looking at.

## ⚠ Why chain 186 and not the newer chain 187

The parcel that created this directory was briefed to freeze sigil's tip at the time, `dd371e3b` —
`freeze: scroll-and-section-clamps`, chain 187, `aeon_rev ec6a4791`. It does not, and the reason is
measured, not argued:

* Chain 187 is **FROZEN-BUT-UNATTESTED**. Aeon's post-freeze strict suite went **red — 8 failures, 7 of
  them one cross-seam symbol** — and a **superseding freeze is expected** once sigil's fixes land. Aeon
  expects the ROM bytes to be identical across that superseding freeze but **would not promise it**.
* Chain 187's `s4.debug.bin` is **byte-identical to the Aeon build that broke our four replay rows**
  (sha256 `951cf960…62707d`, Aeon's on-disk ROM at 22:35). Freezing it would have frozen the breakage:
  measured directly, `ORACLE_AEON_DIR=<chain-187 copy> cargo test -p oracle-replay --test
  replay_real_artifacts` gives **9 passed / 4 failed**, the same four rows, because the ROM's embedded
  replay fixture disagrees with the ROM's own game code. There is no stale constant on our side to fix:
  the game itself raises `REPLAY DESYNC` at ring 0 (recorded `490164326`, produced `221728870`).
* Chain 186 is the last freeze whose embedded fixture is coherent: the same command against it gives
  **13 passed / 0 failed**, with our code unchanged.

Adopting a known-red upstream build as our own regression baseline would give us four permanently red
rows that signal nothing. So this pin is the last coherent freeze, taken by the same recipe and verified
to the same standard (ROM bytes equal to a committed sigil golden; listings from a clean tree at that
freeze's own `aeon_rev`).

**Open question, deliberately left visible:** the chain-187 desync is either Aeon's — a replay fixture not
re-recorded after the scroll/section-clamp change — or **ours**, an emulator inaccuracy that the new
clamp code exposes. This freeze does not settle that, and pinning 186 must not be allowed to bury it.
Reproduce the failing case at any time with:

```sh
ORACLE_AEON_DIR=/home/volence/sonic_hacks/aeon cargo test -p oracle-replay --test replay_real_artifacts
```

## The bytes, as committed

sha256 of every artifact in this directory as committed. A later reader can check the bytes without
trusting any of the story above.

| file | bytes | sha256 |
|---|---:|---|
| `s4.bin` | 719,235 | `b0873bed491c16b97f0cd1a1e7dba0acbebdb8e55276e2fd092b0e1705be3351` |
| `s4.debug.bin` | 736,095 | `75e9f4d4b7fb8ab0f9880b43d20622abef4ef1e4b672694ae6921f71619fcf7a` |
| `s4.lst` | 280,300 | `98cc5b60500e81c4c9bbdef7b6fd5b86878b368d02af55cbf2966abca2c9b8b6` |
| `s4.debug.lst` | 329,345 | `d478dec2c7a771d0485a6dab9b52c48a1f63323439da10ab018af7d64d4feccb` |
| `demo.lst` | 175,771 | `7f4c41fee539050f6b6bf43d8993f6f050da8e34b2d210bc82b04e6ea73db88d` |
| `demo.debug.lst` | 203,387 | `2c4ffa8e7d7b7087dc34caa01d190b0499d5aa4870a0d0e8a9c5fe60fdaedb60` |

Reproduce with:

```sh
sha256sum fixtures/aeon/*.bin fixtures/aeon/*.lst
```

## Moving the pin — deliberately, never silently

**The rule: this pin never moves to make a red test go green.** A number re-pinned to "whatever Aeon last
built" is a pin that cannot fail and therefore detects nothing. If these tests go red, the first question
is *what changed and is it ours* — never *what value would make it pass*.

Move the pin only when we have decided to adopt a newer Aeon build — typically once a superseding sigil
freeze lands **and is attested**. The steps:

1. Pick the sigil freeze commit to adopt. Read its `crates/sigil-harness/golden/provenance.toml` tip entry
   for `aeon_rev`, the freeze name and chain number, and whether it is attested.
2. Take the two ROMs out of that commit as git blobs
   (`git -C ../sigil show <rev>:crates/sigil-harness/golden/s4.bin`, and `s4.debug.bin`) — not out of a
   working tree.
3. Take the four listings from an Aeon tree that built that exact freeze. Prefer a **clean** tree whose
   `HEAD` equals the freeze's `aeon_rev`, and record which tree it was.
4. **Re-verify the joint immediately**: re-hash that tree's on-disk `s4.bin` / `s4.debug.bin` and confirm
   they equal the golden blobs you just took. If they differ you have a mismatched ROM/listing pair —
   **stop**. Do not resolve it by taking the ROM from the working tree to match the listing.
5. Re-run `cargo test --workspace`. The tick counts pinned in
   `crates/oracle-replay/tests/replay_real_artifacts.rs` (`Fixture::Ojz` 1721, `Fixture::OjzSlide` 2350)
   are ROM-derived — the replay fixture is embedded in the ROM — so if the new ROM yields different
   values, re-derive them **once** and name the cause in the commit message. If they are unchanged, say so
   explicitly rather than letting a reader assume you checked.
6. Update this file: every sha256, the sigil revision, the `aeon_rev`, the capture time, the source tree,
   and the status notes.
7. Commit the artifact bytes, this file, and any tick-count change **together**, with a message naming
   which build you moved to and why.

## Frozen history

| when | sigil freeze | `aeon_rev` | listings from | note |
|---|---|---|---|---|
| 2026-08-30 | `5af70797` — `fg-left-edge-borrow`, chain 186 | `def98ee5` | `.aeon-ref-186` (clean, at `def98ee5`) | initial freeze — ends the live-tree dependency on Aeon. Chain 187 (`dd371e3b`) was the briefed tip but is unattested and desyncs the replay fixture; see above. |
