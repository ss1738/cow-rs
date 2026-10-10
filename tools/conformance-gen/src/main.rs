//! Generates `crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json`
//! from `cowprotocol/services`' own production `model::order`/`model::signature`
//! code, pinned via git dependency (see `Cargo.toml`). No services-side
//! change and no services PR are required for this to work.
//!
//! Covers the agreed 19-order-vector / 38-signing-scheme-check matrix:
//! OrderKind (Buy/Sell) x SellTokenSource (Erc20/External/Internal) x
//! BuyTokenDestination (Erc20/Internal) = 12 base vectors, each checked
//! under both Eip712 and EthSign, plus 7 boundary vectors (receiver in
//! {None, zero, nonzero}, amount in {1, U256::MAX}, validTo in {1,
//! u32::MAX}).
//!
//! The EthSign digest comes from `EcdsaSignature::recover(...).message`
//! — a public field on services' own production `Recovered` struct — not
//! from an independently hand-rolled EIP-191 wrapper. `recover()` itself
//! computes that digest via `hashed_signing_message`, which internally
//! calls the private `hashed_ethsign_message`; routing through `recover`
//! is the only way to get that exact private-fn-computed value from
//! outside `model` without reimplementing it.
//!
//! Run with `cargo run -p conformance-gen` to regenerate the fixture.

use {
    alloy_primitives::{Address, U256, address, keccak256},
    alloy_signer_local::PrivateKeySigner,
    app_data::AppDataHash,
    model::{
        DomainSeparator,
        order::{BuyTokenDestination, OrderData, OrderKind, SellTokenSource},
        signature::{EcdsaSignature, EcdsaSigningScheme},
    },
    serde::Serialize,
    std::{fs, path::Path},
};

/// Bump whenever the vector shape or generation semantics change. The
/// consumer test asserts this matches, so a schema drift fails loudly
/// instead of silently comparing mismatched fields.
const SCHEMA_VERSION: u32 = 1;

const SERVICES_PIN: &str = "4eda54b5b42cb89bf69d4c659f589da4481b258e";

// Canonical GPv2Settlement CREATE2 deployment; identical on every CoW chain.
const SETTLEMENT: Address = address!("9008D19f58AAbD9eD0D60971565AA8510560ab41");
const CHAIN_ID: u64 = 1;

// Hardhat/Anvil account #0 default private key (well-known test key,
// also documented as the playground mnemonic's first account in the
// services repo's own CLAUDE.md). Used only to produce reproducible
// signature vectors; holds no funds anywhere.
const TEST_PRIVATE_KEY: [u8; 32] =
    hex_literal::hex!("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80");

#[derive(Serialize)]
struct Fixture {
    schema_version: u32,
    services_sha: String,
    settlement: String,
    chain_id: u64,
    signer: String,
    order_vector_count: usize,
    signing_scheme_check_count: usize,
    vectors: Vec<Vector>,
}

#[derive(Serialize)]
struct Vector {
    id: &'static str,
    sell_token: String,
    buy_token: String,
    receiver: Option<String>,
    sell_amount: String,
    buy_amount: String,
    valid_to: u32,
    app_data: String,
    fee_amount: String,
    kind: &'static str,
    partially_fillable: bool,
    sell_token_balance: &'static str,
    buy_token_balance: &'static str,
    domain_separator: String,
    hash_struct: String,
    eip712_digest: String,
    ethsign_digest: String,
    eip712_signature: SigParts,
    eip712_recovered_owner: String,
    ethsign_signature: SigParts,
    ethsign_recovered_owner: String,
    uid: String,
}

#[derive(Serialize)]
struct SigParts {
    r: String,
    s: String,
    v: u8,
}

fn hex32(b: impl AsRef<[u8]>) -> String {
    format!("0x{}", const_hex::encode(b.as_ref()))
}

fn hex_addr(a: Address) -> String {
    format!("0x{}", const_hex::encode(a.as_slice()))
}

struct Case {
    id: &'static str,
    sell_token: Address,
    buy_token: Address,
    receiver: Option<Address>,
    sell_amount: U256,
    buy_amount: U256,
    valid_to: u32,
    app_data: [u8; 32],
    fee_amount: U256,
    kind: OrderKind,
    partially_fillable: bool,
    sell_token_balance: SellTokenSource,
    buy_token_balance: BuyTokenDestination,
}

fn base(id: &'static str) -> Case {
    Case {
        id,
        sell_token: Address::repeat_byte(0x01),
        buy_token: Address::repeat_byte(0x02),
        receiver: Some(Address::repeat_byte(0x03)),
        sell_amount: U256::from(0x0246ddf97976680000_u128),
        buy_amount: U256::from(0xb98bc829a6f90000_u128),
        // Deliberately not a boundary value: the validTo boundary vectors
        // below (1, u32::MAX) must stay distinct from every base vector.
        valid_to: 1_893_456_000,
        app_data: *keccak256(b"cow-rs conformance vector"),
        fee_amount: U256::from(0x0de0b6b3a7640000_u128),
        kind: OrderKind::Sell,
        partially_fillable: false,
        sell_token_balance: SellTokenSource::Erc20,
        buy_token_balance: BuyTokenDestination::Erc20,
    }
}

/// Exactly the agreed 19-order-vector matrix. Each entry here becomes 2
/// signing-scheme checks (Eip712 + EthSign) in the emitted fixture, for
/// 38 checks total.
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    // 12 base vectors: full cross of OrderKind x SellTokenSource x
    // BuyTokenDestination at otherwise-default field values.
    for kind in [OrderKind::Sell, OrderKind::Buy] {
        for sell_bal in [
            SellTokenSource::Erc20,
            SellTokenSource::External,
            SellTokenSource::Internal,
        ] {
            for buy_bal in [BuyTokenDestination::Erc20, BuyTokenDestination::Internal] {
                let id: &'static str = Box::leak(
                    format!(
                        "{}_{}_{}",
                        match kind {
                            OrderKind::Sell => "sell",
                            OrderKind::Buy => "buy",
                        },
                        match sell_bal {
                            SellTokenSource::Erc20 => "sb_erc20",
                            SellTokenSource::External => "sb_external",
                            SellTokenSource::Internal => "sb_internal",
                        },
                        match buy_bal {
                            BuyTokenDestination::Erc20 => "bb_erc20",
                            BuyTokenDestination::Internal => "bb_internal",
                        },
                    )
                    .into_boxed_str(),
                );
                cases.push(Case {
                    id,
                    kind,
                    sell_token_balance: sell_bal,
                    buy_token_balance: buy_bal,
                    ..base(id)
                });
            }
        }
    }

    // +3 receiver boundary vectors, at the base kind/balance combo.
    for (label, receiver) in [
        ("receiver_none", None),
        ("receiver_zero", Some(Address::ZERO)),
        ("receiver_nonzero", Some(Address::repeat_byte(0x07))),
    ] {
        let id: &'static str = Box::leak(label.to_string().into_boxed_str());
        cases.push(Case {
            id,
            receiver,
            ..base(id)
        });
    }

    // +2 amount boundary vectors (sell_amount == buy_amount == fee_amount
    // at the boundary, so the boundary is exercised in every amount
    // field the struct hash reads).
    for (label, amount) in [
        ("amount_one", U256::from(1u8)),
        ("amount_u256_max", U256::MAX),
    ] {
        let id: &'static str = Box::leak(label.to_string().into_boxed_str());
        cases.push(Case {
            id,
            sell_amount: amount,
            buy_amount: amount,
            fee_amount: amount,
            ..base(id)
        });
    }

    // +2 validTo boundary vectors.
    for (label, valid_to) in [("valid_to_one", 1u32), ("valid_to_u32_max", u32::MAX)] {
        let id: &'static str = Box::leak(label.to_string().into_boxed_str());
        cases.push(Case {
            id,
            valid_to,
            ..base(id)
        });
    }

    assert_eq!(
        cases.len(),
        19,
        "coverage matrix must contain exactly 19 order vectors"
    );
    cases
}

fn to_vector(case: Case, signer: &PrivateKeySigner, domain: &DomainSeparator) -> Vector {
    let order = OrderData {
        sell_token: case.sell_token,
        buy_token: case.buy_token,
        receiver: case.receiver,
        sell_amount: case.sell_amount,
        buy_amount: case.buy_amount,
        valid_to: case.valid_to,
        app_data: AppDataHash(case.app_data),
        fee_amount: case.fee_amount,
        kind: case.kind,
        partially_fillable: case.partially_fillable,
        sell_token_balance: case.sell_token_balance,
        buy_token_balance: case.buy_token_balance,
    };

    let struct_hash = order.hash_struct();

    let eip712_sig = EcdsaSignature::sign(EcdsaSigningScheme::Eip712, domain, &struct_hash, signer);
    let eip712_recovered = eip712_sig
        .recover(EcdsaSigningScheme::Eip712, domain, &struct_hash)
        .expect("eip712 signature must recover");

    let ethsign_sig =
        EcdsaSignature::sign(EcdsaSigningScheme::EthSign, domain, &struct_hash, signer);
    let ethsign_recovered = ethsign_sig
        .recover(EcdsaSigningScheme::EthSign, domain, &struct_hash)
        .expect("ethsign signature must recover");

    // Both digests come from the `Recovered.message` public field, which
    // `recover()` computed via services' own (private, internally-called)
    // `hashed_signing_message` — not from a digest we reconstruct by hand.
    let eip712_digest = eip712_recovered.message;
    let ethsign_digest = ethsign_recovered.message;

    let uid = order.uid(domain, eip712_recovered.signer);

    Vector {
        id: case.id,
        sell_token: hex_addr(case.sell_token),
        buy_token: hex_addr(case.buy_token),
        receiver: case.receiver.map(hex_addr),
        sell_amount: case.sell_amount.to_string(),
        buy_amount: case.buy_amount.to_string(),
        valid_to: case.valid_to,
        app_data: hex32(case.app_data),
        fee_amount: case.fee_amount.to_string(),
        kind: match case.kind {
            OrderKind::Sell => "sell",
            OrderKind::Buy => "buy",
        },
        partially_fillable: case.partially_fillable,
        sell_token_balance: match case.sell_token_balance {
            SellTokenSource::Erc20 => "erc20",
            SellTokenSource::External => "external",
            SellTokenSource::Internal => "internal",
        },
        buy_token_balance: match case.buy_token_balance {
            BuyTokenDestination::Erc20 => "erc20",
            BuyTokenDestination::Internal => "internal",
        },
        domain_separator: hex32(domain.0),
        hash_struct: hex32(struct_hash),
        eip712_digest: hex32(eip712_digest),
        ethsign_digest: hex32(ethsign_digest),
        eip712_signature: SigParts {
            r: hex32(eip712_sig.r),
            s: hex32(eip712_sig.s),
            v: eip712_sig.v,
        },
        eip712_recovered_owner: hex_addr(eip712_recovered.signer),
        ethsign_signature: SigParts {
            r: hex32(ethsign_sig.r),
            s: hex32(ethsign_sig.s),
            v: ethsign_sig.v,
        },
        ethsign_recovered_owner: hex_addr(ethsign_recovered.signer),
        uid: format!("0x{}", const_hex::encode(uid.0)),
    }
}

fn main() {
    let signer = PrivateKeySigner::from_bytes(&TEST_PRIVATE_KEY.into()).unwrap();
    let domain = DomainSeparator::new(CHAIN_ID, SETTLEMENT);

    let vectors: Vec<Vector> = cases()
        .into_iter()
        .map(|c| to_vector(c, &signer, &domain))
        .collect();
    let order_vector_count = vectors.len();
    let signing_scheme_check_count = order_vector_count * 2;

    let fixture = Fixture {
        schema_version: SCHEMA_VERSION,
        services_sha: SERVICES_PIN.to_string(),
        settlement: hex_addr(SETTLEMENT),
        chain_id: CHAIN_ID,
        signer: hex_addr(signer.address()),
        order_vector_count,
        signing_scheme_check_count,
        vectors,
    };

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json");
    fs::write(&path, serde_json::to_string_pretty(&fixture).unwrap()).unwrap();
    println!(
        "wrote {} order vectors ({} signing-scheme checks) to {}",
        fixture.order_vector_count,
        fixture.signing_scheme_check_count,
        path.display()
    );
}
