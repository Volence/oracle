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
| Contract repo commit (`HEAD` at vendor time) | `90178fce` — *"contract: CR-15 — JSON-RPC 2.0 mandates a null error id and the schema forbade it"* (2026-08-15) |
| Last commit that touched the schema | `90178fce` — same commit |
| SHA-256 | `b6fd1ff6f79ecd03f2968bce6b69f188a394c17e550967835f66ad2ce4b7a200` |
| Bytes | 30075 |
| Vendored on | 2026-08-15 |

### What this re-vendor adopted

Two contract commits, in the order they were made:

- **`28ef4bb` — CR-10 adopted** (`protocol.md` §11.3): `emulator/pixel_attribution` gains a §6 row, three
  normative behaviours in prose, and a schema entry. The schema goes from 9 methods to 10. Nothing existing
  changed; the diff is insertion-only.
- **`04a67bc` — CR-15 adopted** (`protocol.md` §11.4): `errorResponse.id` now accepts `null`, which
  JSON-RPC 2.0 §5 **mandates** for a response whose request id could not be detected — **restricted by an
  `if`/`then` to `-32700` and `-32600`**, the only two codes decided before a request object exists. On any
  other code a real id was available to echo, so a null one is a correlation bug. `$defs/id` is deliberately
  unchanged, so null stays illegal on a request and on a success response. The adopted shape therefore
  preserves all four fences the harness had already built around its own allowance.

**CR-15's registered divergence is therefore retired in this commit** — the mechanism working as designed:
the ruling landed upstream, the copy was refreshed, and
`every_registered_divergence_is_still_live` would have failed had the entry been left behind. CR-14
(`lookup_symbol.otherMatches`) is **not** ruled and stays registered.

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
