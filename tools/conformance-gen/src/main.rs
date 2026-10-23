//! Generates the signed-order conformance fixture used to check `cow-rs`'s
//! EIP-712/EthSign hashing and signing against the real implementation in
//! `cowprotocol/services`, at the commit pinned in `Cargo.toml`.
//!
//! The fixture covers every combination of order kind, sell-token source and
//! buy-token destination, plus a handful of edge cases for the receiver,
//! amount and validTo fields, each signed and recovered under both signing
//! schemes. See `assert_eq!` in `cases()` for the exact vector count.
//!
//! Run with `cargo run --locked --manifest-path tools/conformance-gen/Cargo.toml`.

use {
    alloy_primitives::{Address, B256, U256, address, keccak256},
    alloy_signer_local::PrivateKeySigner,
    app_data::AppDataHash,
    model::{
        DomainSeparator,
        order::{BuyTokenDestination, OrderData, OrderKind, OrderUid, SellTokenSource},
        signature::{EcdsaSignature, EcdsaSigningScheme},
    },
    serde::Serialize,
    std::{fs, path::Path},
};

/// Increment when the fixture structure or generation rules change.
const SCHEMA_VERSION: u32 = 1;

// Must match both services dependency revisions in Cargo.toml.
const SERVICES_PIN: &str = "df6a14b12eab016f1ae939aa5b666658208b0bc0";

// GPv2Settlement address used for these mainnet vectors.
const SETTLEMENT: Address = address!("9008D19f58AAbD9eD0D60971565AA8510560ab41");
const CHAIN_ID: u64 = 1;

// Hardhat account 0 test key. Do not use it for funded accounts.
const TEST_PRIVATE_KEY: [u8; 32] =
    hex_literal::hex!("ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80");

#[derive(Serialize)]
struct Fixture {
    schema_version: u32,
    services_sha: String,
    settlement: Address,
    chain_id: u64,
    signer: Address,
    order_vector_count: usize,
    signing_scheme_check_count: usize,
    vectors: Vec<Vector>,
}

// Holds `model::order::OrderData` and `model::signature::EcdsaSignature`
// directly (both already derive `Serialize` with the casing and encoding
// this fixture needs) instead of re-declaring their fields as `String`.
#[derive(Serialize)]
struct Vector {
    id: String,
    #[serde(flatten)]
    order: OrderData,
    domain_separator: B256,
    hash_struct: B256,
    eip712_digest: B256,
    ethsign_digest: B256,
    eip712_signature: EcdsaSignature,
    eip712_recovered_owner: Address,
    ethsign_signature: EcdsaSignature,
    ethsign_recovered_owner: Address,
    uid: OrderUid,
}

// `Case` only adds a human-readable `id` on top of the `OrderData` the
// pinned `model` crate already defines; it does not re-declare `OrderData`'s
// fields.
struct Case {
    id: String,
    order: OrderData,
}

fn base_order() -> OrderData {
    OrderData {
        sell_token: Address::repeat_byte(0x01),
        buy_token: Address::repeat_byte(0x02),
        receiver: Some(Address::repeat_byte(0x03)),
        sell_amount: U256::from(0x0002_46dd_f979_7668_0000_u128),
        buy_amount: U256::from(0xb98b_c829_a6f9_0000_u128),
        // Deliberately not a boundary value: the validTo boundary vectors
        // below (1, u32::MAX) must stay distinct from every base vector.
        valid_to: 1_893_456_000,
        app_data: AppDataHash(*keccak256(b"cow-rs conformance vector")),
        fee_amount: U256::from(0x0de0_b6b3_a764_0000_u128),
        kind: OrderKind::Sell,
        partially_fillable: false,
        sell_token_balance: SellTokenSource::Erc20,
        buy_token_balance: BuyTokenDestination::Erc20,
    }
}

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
                let id = format!(
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
                );
                cases.push(Case {
                    id,
                    order: OrderData {
                        kind,
                        sell_token_balance: sell_bal,
                        buy_token_balance: buy_bal,
                        ..base_order()
                    },
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
        let id = label.to_string();
        cases.push(Case {
            id,
            order: OrderData {
                receiver,
                ..base_order()
            },
        });
    }

    // +2 amount boundary vectors (sell_amount == buy_amount == fee_amount
    // at the boundary, so the boundary is exercised in every amount
    // field the struct hash reads).
    for (label, amount) in [
        ("amount_one", U256::from(1u8)),
        ("amount_u256_max", U256::MAX),
    ] {
        let id = label.to_string();
        cases.push(Case {
            id,
            order: OrderData {
                sell_amount: amount,
                buy_amount: amount,
                fee_amount: amount,
                ..base_order()
            },
        });
    }

    // +2 validTo boundary vectors.
    for (label, valid_to) in [("valid_to_one", 1u32), ("valid_to_u32_max", u32::MAX)] {
        let id = label.to_string();
        cases.push(Case {
            id,
            order: OrderData {
                valid_to,
                ..base_order()
            },
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
    let order = case.order;
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

    assert_eq!(eip712_recovered.signer, signer.address());
    assert_eq!(ethsign_recovered.signer, signer.address());

    let uid = order.uid(domain, eip712_recovered.signer);

    Vector {
        id: case.id,
        domain_separator: B256::from(domain.0),
        hash_struct: B256::from(struct_hash),
        eip712_digest: eip712_recovered.message,
        ethsign_digest: ethsign_recovered.message,
        eip712_signature: eip712_sig,
        eip712_recovered_owner: eip712_recovered.signer,
        ethsign_signature: ethsign_sig,
        ethsign_recovered_owner: ethsign_recovered.signer,
        uid,
        order,
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
        settlement: SETTLEMENT,
        chain_id: CHAIN_ID,
        signer: signer.address(),
        order_vector_count,
        signing_scheme_check_count,
        vectors,
    };

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/cowprotocol-signing/tests/fixtures/conformance_vectors.v1.json");
    let mut json = serde_json::to_string_pretty(&fixture).unwrap();
    json.push('\n');
    fs::write(&path, json).unwrap();
    println!(
        "wrote {} order vectors ({} signing-scheme checks) to {}",
        fixture.order_vector_count,
        fixture.signing_scheme_check_count,
        path.display()
    );
}
