# Vendored contract artifacts — provenance

`bus-protocol.schema.json` in this directory is a **verbatim copy** of the Aether wire schema from the
contract repo. It is vendored, not read from the sibling checkout at test time, so the test suite is
hermetic: it compiles against a fixed schema and produces the same verdict on a machine that has no
`empyrean/` checkout at all.

The copy is not allowed to rot. `tests/schema_conformance.rs` re-reads the upstream file when it can find
it and asserts the two are **byte-identical**; a contract edit therefore turns this suite red and forces
an explicit re-vendor commit. That commit is the auditable record of "we adopted contract revision X".

## Current copy

| | |
|---|---|
| Source | `empyrean/contract/schema/bus-protocol.schema.json` |
| Contract repo commit (`HEAD` at vendor time) | `d45dc87` — *"contract: CR-16 — five of §11.5's own registrations never reached the schema"* (2026-08-15) |
| Last commit that touched the schema | `d45dc87` — same commit |
| SHA-256 | `ba5a2563761f323f6731c392fcf79339bbdc02800caf4b9b796c43e82ec23490` |
| Bytes | 62134 |
| Vendored on | 2026-08-15 |

### What this re-vendor adopted

One contract commit, **`f309cc8`** — the result-key ruling (`protocol.md` §11.5), which nearly doubled the
schema (30,075 → 59,356 bytes). Four things landed, and three of them change our wire:

- **12 new result fragments.** Every advertised method now has one, so `tests/schema_conformance.rs`'s
  `UNCOVERED_METHODS` goes from 12 entries to **none**. This is the direction that counts: the fragments
  were written upstream from the ruling, not derived here from what we emit.
- **§4 rewritten, and `lookup_symbol` changed on three counts.** `name` is the identifying spelling on
  every branch and MUST round-trip — `$defs/symbolName` rejects a `+$hex` displacement suffix by pattern,
  which is what our address direction used to emit. `rawName` is **struck**. `exact` becomes REQUIRED and
  present on both name-direction branches. `otherMatches` becomes `$defs/boundedList` with one pinned item
  shape and **no `cursor`, no `nextCursor`**.
- **§2.4, new: the shared result conventions** — `caveat` specified once for the whole bus, and the
  bounded-list rule (a)–(d). Clause (b) is why `rpc::bounded_array` stopped emitting a continuation token:
  a method that accepts no cursor param must not emit one.
- **§8 item 20, new:** a server's conformance suite MUST close every result against its fragment, as
  `unevaluatedProperties: false` applied **at test time** and deliberately not published. Implemented in
  `common::schema::closed`.

**CR-14's registered divergence is retired in this commit** — the mechanism working as designed: the ruling
landed upstream, the copy was refreshed, and `every_registered_divergence_is_still_live` failed on the next
run because the shape it registered was no longer rejected. That failure is the reason the entry is deleted
rather than quietly wrong.

**And CR-16 was raised by this re-vendor**, on the first run with item 20's closure live: five keys that
§11.5's own prose registers by name — `initialize.limits`, `initialize.methodSummaries`,
`read_memory.region`, `read_memory.symbolDisp`, `read_memory.caveat` — never reached their fragments. Two
fragments out of 22 were left behind by a large amendment. Registered, not silenced; see
`docs/2026-08-14-aether-change-requests.md`.

## Re-vendoring

When the freshness test goes red:

```sh
cp /home/volence/sonic_hacks/empyrean/contract/schema/bus-protocol.schema.json \
   crates/oracle-aether/tests/contract/bus-protocol.schema.json
sha256sum crates/oracle-aether/tests/contract/bus-protocol.schema.json
git -C /home/volence/sonic_hacks/empyrean log -1 --format='%H %s' -- contract/schema/bus-protocol.schema.json
```

Update the table above with the new commit and hash, then run `cargo test -p oracle-aether`. If the new
schema rejects messages the server sends, **that is the point** — contract §8 item 15: where a server's
shape and the schema disagree, the server changes. Never the wire silently.

## Locating the upstream copy

The freshness test looks for the sibling checkout, in order:

1. `$AETHER_CONTRACT_SCHEMA` — an explicit path to the upstream schema file.
2. Ancestor directories of `CARGO_MANIFEST_DIR`, each probed for
   `empyrean/contract/schema/bus-protocol.schema.json` (this finds it from a normal checkout *and* from a
   `.claude/worktrees/…` worktree, whose depth differs).

If none hit, the test **fails loudly** rather than passing — see the comment on
`the_vendored_schema_is_byte_identical_to_the_upstream_contract` for why, and for the
`AETHER_CONTRACT_OPTIONAL=1` escape hatch.

### CR-16, adopted hours after `f309cc8` and retired the same day

`d45dc87` adds five `properties` entries across two fragments — `initialize.limits`,
`initialize.methodSummaries`, `read_memory.region`/`.symbolDisp`/`.caveat` — all of which `protocol.md`
already **registered in prose** and none of which reached the schema. `limits` joins `initialize`'s
`required`, `region` joins `read_memory`'s. No prose changed; the prose was already right.

It was found by §8 item 20's closure on its first run, in the document rather than in the server. Its
registry entries and their key-checkers are gone from `tests/common/schema.rs`, and that retirement was
**forced, not remembered**: those checkers *lift* their key out of the payload before validating it, so the
moment the schema required `limits`, lifting it made it missing — and every checkpoint test went red on the
handshake. An allowance that outlives its divergence does not go stale quietly; it starts causing the
failure it was written to suppress, in tests unrelated to it.

One fixture moved with it: `schema_conformance.rs`'s `good_read_memory_reply()` omitted `region` and so
stopped being conformant the moment the fragment declared it — the positive control catching its own drift,
which is the only reason the rejection controls beneath it stayed meaningful.
