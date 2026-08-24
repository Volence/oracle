# His ephemeral-instance ruling meets the interactive surface — one gap, and the shape of the fix

**2026-08-24.** Written the moment the ruling reached this lane, because it lands squarely in this
component and the obvious next action — stop the standing server — would remove a capability with
nothing replacing it.

## The ruling, verified rather than relayed

Owner, in his own words (empyrean `7861b79`, `d-4-outcome`, verified here as an ancestor of their
`origin/main`):

> "Yeah whenever something needs the emulator have it open an instance no? then close when done or
> something"

The hub recorded it under DECISIONS rule 8b — as **its own option**, not mapped onto one of the
three they offered, all of which assumed a standing instance and argued only about how it stays
alive. **He removed the thing that needed supervising instead.** That is the better answer and this
lane is not relitigating it.

## The ruling is already proven — for automation

aeon's gates spawn their own emulator on their own private socket and never dial the shared path
(`oracle-old/linux-port/harness/launcher.py`: `mkdtemp(prefix="oracle-harness-")`, isolated
`XDG_RUNTIME_DIR`, `sock = tmp/"oracle.sock"`, every gate passing `socket_path=sock`). That was
established firsthand on 2026-08-24 when it refuted this repo's own "day-one breakage" claim. So
per-instance is **the existing automation posture generalised**, not new ground. The hub is right
about that.

## THE GAP — the interactive MCP surface cannot open an instance, and nothing else will

**Measured, not inferred.** The shim is two files and neither can spawn a server:

- `oracle-old/linux-port/mcp/oracle-mcp` — a 26-line `/bin/sh` wrapper whose only action is
  `exec "$VENV/bin/python3" "$DIR/oracle_mcp.py" "$@"`.
- `oracle-old/linux-port/mcp/oracle_mcp.py` — `_get_bus()` constructs a `BusClient` and calls
  `connect()`. Grepped its control flow for `subprocess`, `Popen`, `os.exec*`, `os.spawn*`:
  **zero hits.** The only matches for "exec"/"pause" in the file are docstrings.

**So the interactive surface is a pure client.** Under a strict reading of the ruling — no standing
instance — every `mcp__oracle__*` call in all six sessions returns `ENOENT` forever, because the
thing that "needs the emulator" is a long-lived MCP client with no ability to open one. The people
most affected are the owner and every lane driving the emulator by hand.

**This is not an argument against his ruling.** It is one missing piece between the ruling and the
surface that has to honour it, and the piece is small.

## The fix that IS his ruling, applied to the client that cannot do it yet

**Make the shim spawn a per-session instance on first call and reap it on exit**, on a per-session
socket path. Every mechanism this needs already exists:

- `resolve_socket_path()` honours `$ORACLE_SOCKET` **first**, before the `XDG_RUNTIME_DIR` directory
  test (`empyrean/clients/python/aether.py:36-48`). Per-session paths need no protocol change.
- `Server::bind` probes before binding: a live server answering is `AddrInUse`, nothing answering
  means the file is a corpse and is unlinked. **A stale socket can never block a spawn**, which is
  the property that makes ephemeral instances safe to restart into.
- The server takes `--socket PATH` already (`crates/oracle-aether/src/main.rs`).

That is "open an instance when something needs it, close it when done" — for the one consumer that
structurally cannot today.

## Consequences to book before anyone builds it

1. **▶ It raises the price of our missing signal handling, which was correctly declined before and
   should now be re-priced.** `src/main.rs` parks forever with no `SIGINT`/`SIGTERM` handling, so the
   standalone binary leaves its socket file behind — declined deliberately because catching signals
   needs `signal-hook` or `unsafe libc` against a documented frozen dependency set and a
   `forbid(unsafe_code)` library. **Under a standing instance that is a cosmetic papercut once a
   week. Under ephemeral instances it fires on every session exit** — the hub's "corpse factory",
   and they are right to flag it. Partly defused by `Server::bind`'s unlink-on-bind (each spawn
   cleans its own path), so the residue is narrower than it sounds: a **client** dialling a corpse
   gets `ECONNREFUSED` instead of `ENOENT`. Which is exactly empyrean's open CR-SOCKET question,
   whose ground his ruling has moved.
2. **It simplifies aurora's "which instance is visible" problem rather than creating it.** The hub
   left this open as a design question for oracle and aurora. Under per-session spawn it largely
   answers itself: aurora's live loop spawns and owns **its** instance, and that instance is by
   construction the one the owner is watching, because it is the one their window is driving. The
   question only bites if two clients must share one visible instance, which is the arrangement his
   ruling removes.
3. **`protocol.md` §7.1 names ONE well-known socket path as the reference transport.** Sharing is a
   consequence of a single well-known address, not designed multi-tenancy — the hub's point and it
   is correct. Per-instance work implies per-instance paths, so §7.1 likely needs a sentence saying
   the well-known path is a default rather than the arrangement. **empyrean's to write, not ours.**

## What this lane is doing tonight, and why it is not "ignoring the ruling"

**pid 29281 stays up tonight.** Stopping it is one signal and I am not sending it, for one stated
reason: **until the shim can spawn, retiring the standing instance does not implement his ruling —
it just removes the interactive surface for six sessions and leaves nothing in its place.** The
sequence that honours the ruling is *build the spawn, then retire the standing instance*, and doing
it in the other order produces an outage that looks like compliance.

Recorded here rather than argued in a message, so that a session which boots into a running server
after a ruling that says there should not be one finds the reason instead of the contradiction.
**If the owner wants it down before the spawn exists, that is his call and it is one command.**
