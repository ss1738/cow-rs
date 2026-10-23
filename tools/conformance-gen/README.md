# conformance-gen

Generates `crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json`
from the order and signature code in `cowprotocol/services`.

The services dependencies are pinned to commit
`df6a14b12eab016f1ae939aa5b666658208b0bc0`.

## Coverage

The fixture contains 19 order vectors and 38 signing-scheme checks. The base
matrix covers:

- `OrderKind`: Buy and Sell
- `SellTokenSource`: Erc20, External and Internal
- `BuyTokenDestination`: Erc20 and Internal

Additional vectors cover absent, zero and nonzero receivers; amount values of
1 and `U256::MAX`; and `validTo` values of 1 and `u32::MAX`. Each vector
includes EIP-712 and EthSign results.

The matrix keeps the token addresses, app data, chain ID, settlement contract
and signer fixed. All orders set `partially_fillable` to false. The two amount
boundary vectors set the sell, buy and fee amounts to the same boundary value.
EIP-1271 signatures, presignatures and orderbook acceptance are outside the
scope of this fixture.

The fixture records:

- the order struct hash
- EIP-712 and EthSign digests
- signatures and recovered owners
- the order UID

The schema is `conformance_vectors.v1.schema.json`. Amounts are decimal strings,
addresses and hashes are lowercase hexadecimal strings, and signature `v` is 27
or 28.

The absent and zero-address receiver cases intentionally produce the same hash,
signatures and UID because both encode as the zero address in `OrderData`.

## Regeneration

From the repository root:

```bash
cargo run --locked --manifest-path tools/conformance-gen/Cargo.toml
```

The first build requires Git and network access to fetch the pinned services
repository and crates.io dependencies. After those dependencies are cached,
the generator can run offline with `--offline`.

The fixture is generated just-in-time from the services commit pinned in
`Cargo.toml` and `src/main.rs`; it is **not committed**. `.gitignore` excludes
`crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json`. Run
the command above before any test or CI step that reads it, including the
Milestone 2 consumer test, which is expected to invoke this generator (or a
CI step that runs before it) rather than read a committed file. Only the
hand-authored schema, `conformance_vectors.v1.schema.json`, is committed.

When changing the services revision, update both `rev` entries in `Cargo.toml`
and `SERVICES_PIN` in `src/main.rs`, regenerate `Cargo.lock` without `--locked`,
then run the locked command above to produce a fresh fixture locally.

The generator is excluded from the root workspace because its native services
dependencies do not compile for the workspace's `wasm32-unknown-unknown` CI
target, and so that those dependencies never enter the root `Cargo.lock`
(relevant to the un-committed fixture too: regenerating it never touches the
root lockfile or dependency graph).

No test consumes this fixture in Milestone 1. The consumer test is planned for
Milestone 2.
