# Handoff — the overnight run (2026-08-16)

Follows `docs/2026-08-15-handoff-conformance-and-item19.md`, whose §7 ranked list this session worked
down. **Nothing is pushed** in any of the three repos.

| repo | tip | state |
|---|---|---|
| `oracle-next` | `55cf62b` | committed, **not pushed** |
| `empyrean` | `2d5dac9` | committed, **not pushed** — three amendments, §11.10 – §11.12 |
| `oracle` | `7da5344` | committed, **not pushed** — the MCP client fixes |

Gates on the final tree, run firsthand: `cargo test --workspace` → **EXIT=0, 1426 passed / 0 failed / 36
legs** (session baseline 1392/33); clippy 0 warnings default *and* `--no-default-features`; `cargo fmt
--all --check` clean; **`crates/oracle-core/tests/` still a zero-file diff**, fourth session running.

## What shipped

1. **The MCP validation** (ranked item 1, owner-ruled MCP-first) — and the framing was wrong: the MCP was
   *already* an Aether client and had simply never been pointed at the second implementation. The real
   `oracle_mcp` module drives oracle-next end to end; 15 of 16 shared tools work with the parameters their
   own schemas declare. **Both failures were client bugs**, fixed in `oracle`:
   `read_vram`/`write_vram` declared `addr` as an integer, and the screenshot branch handed the model a
   PPM labelled `image/png`. Both were *true by accident* against the only server the client had ever met.
2. **`emulator/sprites`** (CR-18, §11.10) — the last of the four §8 item-19 violations. **All four are now
   closed.**
3. **`fault_run`** (`examples/fault_run.rs`) — the emulator half of Aeon's replay net, which needed **no
   new emulator capability**: the engine plays its own `ARP0` stream, so the whole job is noticing it
   reached its fault handler. Exit 0 clean / 1 faulted / 2 setup error.
4. **`emulator/play_input`** (CR-19, §11.11) — the pad as a timeline: the pad at frame N is a pure
   function of the timeline and of nothing else.
5. **`emulator/read`** (CR-20, §11.12) — one byte read across the `bus`/`vram`/`cram`/`vsram` spaces.
   **Advertised methods 25 → 28 across the session.** `read_memory` and `read_vram` are
   deprecated-and-kept as exact aliases, so the MCP needs no change at all.

## The three things worth reading even if you skip the rest

**1. Every ranked item's headline claim failed on inspection — three for three.**

- Item 1: "port the MCP onto Aether" — it was already ported; the work was an A/B of two servers against
  one real client, and cost an afternoon rather than an arc.
- Item 3: "retires six in-tree re-implementations" — it is **one**. And its ARP0 justification pointed at
  the *playback* path, where a pad timeline is inert; the pain it cited lives in the *recording* path.
- Capability 1: "collapses six read methods into one" — three of the six are not **address-shaped**. Under
  deprecate-and-keep it is two built methods absorbed plus two never-built rows, and nothing is retired.

These were all ranked from a recon this project wrote itself, and the recon's *conclusions* have held up
every time. It is the supporting counts and mechanisms that have not. **Check the scope of a claim, not
just the claim.**

**2. A correction can be right and still stop one step short.** CR-19's adjudication is the sharpest
lesson of the session: both of the CR's corrections were verified and correct about what they checked, and
each stopped exactly one step early — one examined playback while its evidence lived in recording, the
other objected to a promise in the MCP while the identical promise sat unfixed in our own contract.

**2b. Each of three rulings found its defect in the same place — the CR's evidence, never its design.**
§11.10 struck a quotation that appears in no document; §11.11 named the wrong entry symbol; §11.12 caught
me narrowing `z80_read`'s catalogued bounds against the contract. The scope-correction habit that produced
three good deflated headlines is also what produced three supporting-detail errors while deflating them.

**3. When mutations survive, suspect the instrument.** The first `play_input` test suite passed while
**four of five mutations survived**, including both that matter — merging the held set into the timeline,
and leaving an un-driven port alone. The assertions were fine; the *fixture* was blind (`build_pad_poll`
exposes only Start, and only as a backdrop colour). Adding `testrom::build_pad_log()` — both ports, both
TH phases, written to RAM every poll — took it to **6 of 6**. Two of my own tests died in the process: one
asserted that two 6-frame timelines both ran 6 frames, and one used `state_hash`, an *end-state*
fingerprint, to look for a mid-run transient.

## Owner-owed

**Two product decisions from the MCP validation, deliberately not settled:**

- ~~A model driving oracle-next through the MCP cannot see the screen.~~ **CLOSED** — `emulator/screenshot`
  now emits PNG from an encoder written here (`oracle-aether/src/png.rs`), so the dependency exception was
  not needed. Verified by round-tripping seven images through an *independent* decoder, and end to end
  through the real MCP: `mimeType=image/png`, and the frame is viewable.
- **Nine Aether methods no MCP tool can reach**, including three quarters of the watchpoint surface — from
  the MCP a watch can be **armed and never read**. Three tool-table rows fix that half.

**One Aeon-side ask:** `fault_run` has never seen a real desync, because no build here arms an `ARP0`
stream and no register-write op exists to force one. A debug build with a stream armed, run under
`fault_run`, turns the dead regression net into a one-command CI gate.

**CR-20 is no longer waiting on you** — its adjudication ruled build-now, and deprecate-and-keep means it
churns the MCP not at all. What remains is the *next* pick: ranked item 5 (player build-out) or capability
6 (per-scanline visual state, which needs `F-TRACE-VDPWRITE-MCLK` and in turn unblocks `F-CRAMDOT`).

## Ops

**`/tmp` is mounted `usrquota`, and the per-user quota can exhaust.** It did, mid-session: the suite
failed with `QuotaExceeded` on the checkpoint tests' temp writes, **and the harness's own output capture
silently broke** — every command producing stdout returned exit 1 with no output, while `true` still
succeeded. `df` showed 6.1 GB free, so **free space is not the signal; the quota is**
(`findmnt -no OPTIONS /tmp`; there is no `quota` binary here). The cause was 21 GB of *another* project's
stale scratchpad, which I left alone; reclaiming this project's own stale session scratchpads cleared it
instantly. Workaround while wedged: redirect to a file and read the file.
