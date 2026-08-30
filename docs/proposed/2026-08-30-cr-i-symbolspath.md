# CR-I — `symbolsPath` is the only path this server puts on the wire raw

**Filed by:** oracle lane, 2026-08-30. **Grounds:** found while verifying `parcel/live-tree-readers`
(merged oracle `2aa1704`), and visible in that verification run's own output. Every code anchor below
was re-verified firsthand at `2aa1704` by the filing seat, by symbol and by reading the body, not
transcribed from a note.

## The ask

**`emulator/status`'s `symbolsPath` should be absolute, on the same footing and by the same mechanism
as `romPath`.** No new method, no new field, no shape change. One rider on §6's paths note, and a
description edit to the `symbolsPath` property so the schema stops promising something the servers do
not do.

## 1. What is actually happening

Observed live, in a server this seat spawned on the frozen fixtures and drove with the repo's own
smoke tool:

```
romPath     /home/volence/sonic_hacks/oracle/fixtures/aeon/s4.bin
symbolsPath fixtures/aeon/s4.lst
```

Both were supplied to the binary the same way, in one command line, as sibling relative paths. One
came back absolute and one came back as typed.

## 2. Why this is a contract question and not a local tidy-up

**The schema already promises the symmetry.** `bus-protocol.schema.json:1246-1258`:

* `romPath` — *"Path to the loaded ROM on the SERVER's filesystem — a host-filesystem fact, which D8's
  trusted-local-developer trust model makes unremarkable (see §6's paths note)."*
* `symbolsPath` — *"Path to the loaded listing, **same treatment**."*

**"Same treatment" is doing two jobs and the implementations split along the seam.** It plausibly
means *the same trust-model treatment* — a host-filesystem fact is unremarkable — and it plausibly
means *the same handling*, which is what a reader who has just read §6's "absolute path of the loaded
image" will take it for. One server can satisfy the first reading and violate the second while
passing every schema check, because the schema constrains the type and not the resolution.

That is the whole CR: **the ambiguity is in the contract, so the fix belongs in the contract**, not in
one implementation quietly picking a reading.

## 3. The code, and the part that makes this a miss rather than a decision

`absolutise` (`engine.rs`, `fn absolutise`) is applied on **every ROM route and no symbols route** —
verified by enumerating its call sites rather than by grepping for the key names:

| route | anchor | absolutised? |
|---|---|---|
| `set_rom_path` | `self.rom_path = path.map(\|p\| absolutise(&p))` | **yes** |
| `reload_rom` | `let path = absolutise(&path);` | **yes** |
| `set_symbols` | `self.symbols_path = path;` | no |
| `load_symbols` | `self.symbols_path = Some(path.clone());` | no |

**The argument for fixing `romPath` was written down, and it covers `symbolsPath` verbatim.** From
`set_rom_path`'s own doc comment, describing the 2026-08-26 change under the CR-C ruling (§12.2 item
9):

> *A relative path is not a weaker answer, it is an answer that means something different to every
> reader: a client, a second client, and a log read tomorrow all resolve it against a working
> directory that is not this process's.*

Nothing in that sentence is about ROMs. It is about paths crossing a process boundary. The same
comment then gives the design rule that was also applied to only one key:

> *Done at the boundary rather than in `status` so every route agrees.*

So this is not a case where someone weighed the two keys and chose. **The enumeration was "the key
named in the ruling" rather than "every key with this property"**, and the second key was never
considered. A ruling's own text is a poor enumeration source, because it names the instance that
prompted it.

## 4. What a consumer actually loses

`symbolsPath` is the only field a client has for answering *which listing is this server using*. A
bare `fixtures/aeon/s4.lst` answers that question **only for a reader who happens to share the
server's working directory** — which is exactly the reader who did not need to ask.

The failure is silent in both directions. A consumer that resolves it against its own cwd either
finds nothing (and reports the server has no symbols, which is false) or finds a *different file of
the same name* (and reports agreement it has not established). This lane spent this morning on a
defect of precisely that shape one layer up, where our test fixtures read another team's live tree:
the read succeeded every time and answered about whatever was on disk.

It also defeats the comparison a consumer most wants to make — *are these two servers on the same
listing?* — since two servers with different working directories can report identical strings for
different files, and identical files as different strings.

## 5. Scope, enumerated by role rather than by name

Every field whose **value is a filesystem path** on this server's wire, not every field whose *name*
contains "path":

* **`status.symbolsPath`** — the subject. Raw today; should be absolute.
* **`status.romPath`** — already absolute. No change.
* **`load_symbols`'s reply `path`** (`engine.rs`, `"path": path` in the success object) — echoes the
  caller's spelling. **Should move with `symbolsPath`**, or the same method reports one listing under
  two spellings in one exchange.
* **`screenshot`'s reply `path`** — absolute in the default case because the default is built from
  `std::env::temp_dir()`; raw if the caller supplied a relative `path`. Worth ruling in the same
  breath; the filing seat has no strong view.
* **Error-payload `path` values** (`with_data(json!({"path": path}))`, several sites) — **must stay
  raw, deliberately.** `reload_rom` states the reason at its own call site: *"Absolutised only after
  the read succeeded, so the refusal above still quotes the caller's own spelling back at them — a
  client debugging a bad path wants to see what it sent."* That reasoning is right and this CR must
  not sweep it up. **A refusal describes the request; a success describes the state.**

## 6. The nuance that must survive the change

`absolutise` is `canonicalize`-with-no-fallback **on purpose**, and the reason is recorded at the
function: the string is not always a filesystem path. A hosted embedder sets its image name to
whatever it likes — `"testrom"` — with no file behind it, and prefixing a working directory onto that
label would *manufacture* a path that resolves to nothing and looks authoritative. §6's rule is a
**SHOULD** precisely so that "I cannot honestly say" stays available.

A listing is more likely to be a real file than a ROM label is, but the filing seat sees no reason to
make `symbolsPath` stricter than `romPath` on this point, and one good reason not to: a second rule
is a second thing to get wrong. **Recommend the same SHOULD, the same helper, applied at the same
boundary.**

## 7. Recommendation

Symmetry, argued rather than assumed: the reason `romPath` is absolute is that it crosses a process
boundary to a reader with a different working directory, and `symbolsPath` crosses the same boundary
to the same reader in the same message. The filing seat can construct no reader who is served by
receiving one of the two raw.

**Adopt:** `symbolsPath` SHOULD be the absolute path of the loaded listing, by the same
resolve-or-pass-through rule as `romPath`, applied at the load boundary so every route agrees; and
`load_symbols`'s reply `path` follows it. Error payloads keep the caller's spelling. Replace the
`symbolsPath` description's *"same treatment"* with whatever the ruling decides, in words that name
the resolution rather than gesturing at the neighbouring property — **the gesture is what produced
the divergence.**

**If instead the ruling is that `symbolsPath` stays raw**, that is a coherent position and the CR is
still worth its cost, because the schema must then stop saying "same treatment" and say plainly that
the two keys are resolved differently. **The one outcome to avoid is the current one, where the
contract asserts a symmetry no server provides and each reader discovers it separately.**
