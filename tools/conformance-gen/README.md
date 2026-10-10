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
services_sha = 4eda54b5b42cb89bf69d4c659f589da4481b258e
```

This is the same commit the existing (earlier, exploratory, 57-case)
`services/crates/model/tests/conformance_vectors.rs` prototype and its
`cowprotocol-signing/tests/services_conformance.rs` consumer were already
built and mutation-tested against, so the pin's public API surface
(`OrderData.receiver: Option<Address>`, `OrderKind`, `SellTokenSource`,
`BuyTokenDestination`, `EcdsaSignature::{sign,recover}`) is already
proven to work with this exact generation approach.

**Known, verified fact worth flagging for whoever picks the pin for an
actual PR:** `cowprotocol/services`' `main` has moved since this pin
(checked live on 2026-10-03, current `main` was
`f348ef1465e8ba69cff87edf2abb2e2442256c06`, dated 2026-10-02) and
contains a breaking change relevant to this exact corpus —
`model::order::OrderData.receiver` changed from `Option<alloy_primitives::Address>`
to a new `eth_domain_types::Receiver` type (see `services` commit range
between this pin and current `main`, `crates/model/src/order.rs`). The
`OrderClass` enum was also removed entirely. Bumping the pin past this
point will require reworking the receiver-boundary vectors (currently
`None` / `Some(ZERO)` / `Some(nonzero)`) against the new `Receiver` API
before the generator will compile — this was discovered, not assumed,
while preparing this corpus, and is explicitly left as follow-up work
rather than silently worked around here.

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
- This prototype is local and uncommitted. It has not been submitted as
  a services PR (none is required — see above) and has not been opened
  as a cow-rs PR. Milestone 1 completion requires this to actually be
  committed, reviewed, and accepted upstream in `cow-rs` — that has not
  happened yet.
