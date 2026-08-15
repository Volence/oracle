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
| Contract repo commit (`HEAD` at vendor time) | `18a551ee0d038292a4ac018a5840d9390463d196` — *"contract: give the parity rule the checklist entry its own subject demands"* (2026-08-15) |
| Last commit that touched the schema | `627e5e4a77ed149934264ac890f7b8120443edaf` — *"contract: a checkpoint id is a handle, and the schema wins ties"* (2026-08-15) |
| SHA-256 | `6e6369c9ea78247533015c67feeddde2b0e84cbbf973c72ed4e1e79eb986cec5` |
| Bytes | 19193 |
| Vendored on | 2026-08-15 |

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
