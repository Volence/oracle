# Handoff — the overnight run (2026-08-16)

Follows `docs/2026-08-15-handoff-conformance-and-item19.md`, whose §7 ranked list this session worked
down. **Nothing is pushed** in any of the three repos.

| repo | tip | state |
|---|---|---|
| `oracle-next` | `2f1757d` | committed, **not pushed** |
| `empyrean` | `193906a` | committed, **not pushed** — two amendments, §11.10 and §11.11 |
| `oracle` | `7da5344` | committed, **not pushed** — the MCP client fixes |

Gates on the final tree, run firsthand: `cargo test --workspace` → **EXIT=0, 1412 passed / 0 failed / 35
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
4. **`emulator/play_input`** (CR-19, §11.11) — the pad as a timeline. Advertised methods **25 → 27** across
   the session.
5. **CR-20** (the unified read) written and **left unruled**, with its adjudication running.

## The three things worth reading even if you skip the rest

**1. Every ranked item's headline claim failed on inspection — three for three.**

- Item 1: "port the MCP onto Aether" — it was already ported; the work was an A/B of two servers against
  one real client, and cost an afternoon rather than an arc.
- Item 3: "retires six in-tree re-implementations" — it is **one**. And its ARP0 justification pointed at
  the *playback* path, where a pad timeline is inert; the pain it cited lives in the *recording* path.
- Capability 1: "collapses six read methods into one" — three of the six are **decodes, not reads**. They
  take no address and no length. The collapse is 3 → 1.

These were all ranked from a recon this project wrote itself, and the recon's *conclusions* have held up
every time. It is the supporting counts and mechanisms that have not. **Check the scope of a claim, not
just the claim.**

**2. A correction can be right and still stop one step short.** CR-19's adjudication is the sharpest
lesson of the session: both of the CR's corrections were verified and correct about what they checked, and
each stopped exactly one step early — one examined playback while its evidence lived in recording, the
other objected to a promise in the MCP while the identical promise sat unfixed in our own contract.

**3. When mutations survive, suspect the instrument.** The first `play_input` test suite passed while
**four of five mutations survived**, including both that matter — merging the held set into the timeline,
and leaving an un-driven port alone. The assertions were fine; the *fixture* was blind (`build_pad_poll`
exposes only Start, and only as a backdrop colour). Adding `testrom::build_pad_log()` — both ports, both
TH phases, written to RAM every poll — took it to **6 of 6**. Two of my own tests died in the process: one
asserted that two 6-frame timelines both ran 6 frames, and one used `state_hash`, an *end-state*
fingerprint, to look for a mid-run transient.

## Owner-owed

**Two product decisions from the MCP validation, deliberately not settled:**

- **A model driving oracle-next through the MCP cannot see the screen.** Honest text now replaces the
  corrupt image, but it is not a frame. Fixing it properly means `emulator/screenshot` emitting PNG — one
  line with the `png` crate, against a deliberate policy that `oracle-aether` depends only on
  `oracle-core` + `serde_json`; ~150 lines dependency-free.
- **Nine Aether methods no MCP tool can reach**, including three quarters of the watchpoint surface — from
  the MCP a watch can be **armed and never read**. Three tool-table rows fix that half.

**One Aeon-side ask:** `fault_run` has never seen a real desync, because no build here arms an `ARP0`
stream and no register-write op exists to force one. A debug build with a stream armed, run under
`fault_run`, turns the dead regression net into a one-command CI gate.

**And CR-20 needs your call on sequencing** even if its adjudication says the design is sound: it churns
the MCP you just validated.

## Ops

**`/tmp` is mounted `usrquota`, and the per-user quota can exhaust.** It did, mid-session: the suite
failed with `QuotaExceeded` on the checkpoint tests' temp writes, **and the harness's own output capture
silently broke** — every command producing stdout returned exit 1 with no output, while `true` still
succeeded. `df` showed 6.1 GB free, so **free space is not the signal; the quota is**
(`findmnt -no OPTIONS /tmp`; there is no `quota` binary here). The cause was 21 GB of *another* project's
stale scratchpad, which I left alone; reclaiming this project's own stale session scratchpads cleared it
instantly. Workaround while wedged: redirect to a file and read the file.
