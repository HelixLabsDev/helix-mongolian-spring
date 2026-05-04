#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
blend_root="${repo_root}/reference/blend-contracts-v2"
smoke_dir="${TMPDIR:-/tmp}/helix-real-blend-oracle-smoke"
wrapper_wasm="${repo_root}/target/wasm32v1-none/release/helix_blend_oracle_adaptor.wasm"

cd "${repo_root}"
cargo build -p helix-blend-oracle-adaptor --target wasm32v1-none --release

cd "${blend_root}"
cargo rustc --manifest-path=pool-factory/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release
cargo rustc --manifest-path=backstop/Cargo.toml --crate-type=cdylib --target=wasm32-unknown-unknown --release

rm -rf "${smoke_dir}"
mkdir -p "${smoke_dir}/src"

cat > "${smoke_dir}/Cargo.toml" <<EOF
[package]
name = "helix-real-blend-oracle-smoke"
version = "0.1.0"
edition = "2021"
publish = false

[workspace]
resolver = "2"

[workspace.dependencies]
soroban-sdk = "=22.0.7"
soroban-fixed-point-math = "=1.3.0"
cast = { version = "=0.3.0", default-features = false }
sep-40-oracle = "=1.2.0"
sep-41-token = "=1.2.0"
blend-contract-sdk = "=1.22.0"
moderc3156 = { git = "https://github.com/xycloo/xycloans", rev = "d9a7ae1" }

[dependencies]
soroban-sdk = { workspace = true, features = ["testutils"] }
sep-40-oracle = { workspace = true }
pool = { path = "${blend_root}/pool", features = ["testutils"] }
EOF

cat > "${smoke_dir}/src/lib.rs" <<EOF
#![cfg(test)]

extern crate std;

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    vec, Address, Env, String, Symbol,
};

mod wrapper {
    soroban_sdk::contractimport!(file = "${wrapper_wasm}");
}

const DECIMALS: u32 = 7;
const RESOLUTION: u32 = 300;

#[contracttype]
#[derive(Clone)]
enum MockHelixOracleDataKey {
    Price(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
struct MockPrice {
    price: i128,
    timestamp: u64,
}

#[contract]
struct MockHelixOracle;

#[contractimpl]
impl MockHelixOracle {
    pub fn set_price(env: Env, asset: Address, price: i128, timestamp: u64) {
        env.storage().instance().set(
            &MockHelixOracleDataKey::Price(asset),
            &MockPrice { price, timestamp },
        );
    }

    pub fn lastprice(env: Env, asset: Address) -> (i128, u64) {
        let price = env
            .storage()
            .instance()
            .get::<_, MockPrice>(&MockHelixOracleDataKey::Price(asset))
            .unwrap_or(MockPrice {
                price: 0,
                timestamp: env.ledger().timestamp(),
            });
        (price.price, price.timestamp)
    }

    pub fn decimals(_env: Env, _asset: Address) -> u32 {
        DECIMALS
    }
}

#[test]
fn real_blend_pool_loads_helix_wrapper_price() {
    let env = Env::default();
    env.ledger().set_timestamp(1_700_000_000);
    env.ledger().set_sequence_number(100);
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let hsteth = Address::generate(&env);
    let helix_oracle_id = env.register(MockHelixOracle, ());
    let helix_oracle = MockHelixOracleClient::new(&env, &helix_oracle_id);
    let wrapper_id = env.register(wrapper::WASM, ());
    let wrapper_client = wrapper::Client::new(&env, &wrapper_id);
    let base = wrapper::Asset::Other(Symbol::new(&env, "USD"));
    let assets = vec![&env, wrapper::Asset::Stellar(hsteth.clone())];

    wrapper_client.initialize(
        &admin,
        &helix_oracle_id,
        &base,
        &assets,
        &DECIMALS,
        &RESOLUTION,
    );
    helix_oracle.set_price(&hsteth, &2_345_678_900, &1_700_000_100);

    let pool_id = env.register(
        pool::PoolContract {},
        (
            admin,
            String::from_str(&env, "helix-hsteth"),
            wrapper_id.clone(),
            0_1000000u32,
            4u32,
            1_0000000i128,
            Address::generate(&env),
            Address::generate(&env),
        ),
    );
    let pool_client = pool::PoolClient::new(&env, &pool_id);
    let config = pool_client.get_config();
    assert_eq!(config.oracle, wrapper_id);

    env.as_contract(&pool_id, || {
        let mut blend_pool = pool::PoolState::load(&env);
        assert_eq!(blend_pool.load_price_decimals(&env), DECIMALS);
        assert_eq!(blend_pool.load_price(&env, &hsteth), 2_345_678_900);
    });
}
EOF

cp "${blend_root}/Cargo.lock" "${smoke_dir}/Cargo.lock"

cd "${smoke_dir}"
cargo test --manifest-path "${smoke_dir}/Cargo.toml"
