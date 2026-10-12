# conformance-gen

Generates `crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json`,
the signed-order conformance fixture consumed by
`crates/cowprotocol-signing/tests/conformance_corpus_v1.rs`.

## What this is

A deterministic fixture generator that calls `cowprotocol/services`' own
production `model::order` / `model::signature` code — pulled in as a
pinned git dependency, not copied — to produce the expected values for
the agreed 19-order-vector / 38-signing-scheme-check matrix:

- `OrderData` struct hash
- EIP-712 signing digest
- EthSign signing digest
- ECDSA recovered owner (both schemes)
- Order UID packing

`cow-rs`'s own test (`conformance_corpus_v1.rs`) then independently
recomputes every one of these fields using `cowprotocol-signing`'s own
implementation and asserts an exact match. A divergence means `cow-rs`
would sign or identify an order differently than `services` (and
therefore the real settlement contract) expects.

## Why this needs no services PR and no services-side change

`model` and `app-data` are declared as ordinary git dependencies in
`Cargo.toml`, pinned to a specific `services` commit by `rev`. Cargo
resolves the whole pinned repository and its workspace-internal sibling
crates (`app-data`, `number`, `serde-ext`, `bytes-hex`) automatically from
that one git checkout — nothing needs to be published to crates.io and
nothing in `services` needs to change. This was verified by an actual
`cargo run` against the pin below, not assumed.

## The EthSign digest, specifically

`services::model::signature::hashed_ethsign_message` is a private
function — unreachable from outside the `model` crate. This generator
does **not** reconstruct the EIP-191 "personal_sign" wrapping by hand.
Instead it calls the public `EcdsaSignature::recover(...)`, whose
`Recovered` struct has a public `message: B256` field — `recover()`
computes that field via the same private `hashed_signing_message`
function internally, so reading `.message` back out gives the exact
production-computed digest through a production public API, with no
reimplementation anywhere in this generator.

## Current pin

```
services_sha = df6a14b12eab016f1ae939aa5b666658208b0bc0
```

**Pin updated from the original `4eda54b...`:** that commit's
own `Cargo.toml` uses a multi-line inline table (`sqlx = { version =
"0.8", ... }`) that the cargo bundled with this workspace's pinned
toolchain (rustc 1.91.1, per `.tool-versions`) cannot parse ("newlines
are unsupported in inline tables"), so that pin fails to build under CI's
actual toolchain. `df6a14b...` was verified to build cleanly under
1.91.1 and was substituted for that reason, not for any API-surface
change.

The fixed pin also let `alloy-primitives`/`alloy-signer-local` move to
this workspace's own versions (`.workspace = true`) instead of a
separately pinned 2.x line — an earlier comment here claimed the
workspace's 1.8.3 signer-local was type-incompatible with
`model::signature::sign` at this pin; that was re-tested directly and
found not to hold at `df6a14b...`, so the separate pin was dropped
rather than left in place on an unverified assumption.

## Coverage matrix (19 order vectors, 38 signing-scheme checks)

Full cross of:
- `OrderKind`: Buy, Sell (2)
- `SellTokenSource`: Erc20, External, Internal (3)
- `BuyTokenDestination`: Erc20, Internal (2)

= 12 base order vectors, each checked under both `Eip712` and `EthSign`
= 24 signing-scheme checks.

Plus targeted boundary vectors (at the base kind/balance combination):
- `receiver`: None, zero address, nonzero address (+3)
- `amount` (sell/buy/fee, moved together): 1, `U256::MAX` (+2)
- `validTo`: 1, `u32::MAX` (+2)

= 7 more order vectors, 14 more signing-scheme checks.

**Total: 19 order vectors, 38 signing-scheme checks.**

## Regenerating the fixture

All Rust commands run on the Mac Mini cluster, never on a local MacBook.

```bash
cargo run -p conformance-gen
```

Writes `../../crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json`,
overwriting the previous fixture. Commit the regenerated file alongside
any pin bump.

## Non-goals / explicit scope limits

- This is a signed-order / EIP-712 / EthSign / UID corpus only. It does
  not cover conditional orders, solver benchmarking, AMM quoting, or any
  fuzzing framework.
- EIP-1271 smart-contract signatures, on-chain presign, and full
  orderbook acceptance are out of scope.
- Milestone 1 is submitted for upstream review in PR #63. The Milestone
  2 consumer test, source-lock update, CI verification, and final
  regeneration documentation are intentionally tracked separately.
