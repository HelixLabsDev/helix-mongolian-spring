#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal,
};

use super::{HelixMockOracle, HelixMockOracleClient, DEFAULT_DECIMALS};

struct MockOracleTestFixture<'a> {
    env: Env,
    client: HelixMockOracleClient<'a>,
    admin: Address,
    non_admin: Address,
    asset: Address,
}

impl<'a> MockOracleTestFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.ledger().set(LedgerInfo {
            timestamp: 1_700_000_000,
            protocol_version: 22,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: super::TTL_BUMP + 1_000,
        });

        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);
        let asset = Address::generate(&env);

        let contract_id = env.register(HelixMockOracle, (admin.clone(),));
        let client = HelixMockOracleClient::new(&env, &contract_id);

        Self {
            env,
            client,
            admin,
            non_admin,
            asset,
        }
    }
}

fn set_price_with_auth(
    fixture: &MockOracleTestFixture<'_>,
    signer: &Address,
    asset: &Address,
    price: i128,
    timestamp: u64,
) {
    let invocation = MockAuthInvoke {
        contract: &fixture.client.address,
        fn_name: "set_price",
        args: (asset.clone(), price, timestamp).into_val(&fixture.env),
        sub_invokes: &[],
    };
    let auth = MockAuth {
        address: signer,
        invoke: &invocation,
    };

    fixture
        .client
        .mock_auths(&[auth])
        .set_price(asset, &price, &timestamp);
}

fn set_decimals_with_auth(
    fixture: &MockOracleTestFixture<'_>,
    signer: &Address,
    asset: &Address,
    decimals: u32,
) {
    let invocation = MockAuthInvoke {
        contract: &fixture.client.address,
        fn_name: "set_decimals",
        args: (asset.clone(), decimals).into_val(&fixture.env),
        sub_invokes: &[],
    };
    let auth = MockAuth {
        address: signer,
        invoke: &invocation,
    };

    fixture
        .client
        .mock_auths(&[auth])
        .set_decimals(asset, &decimals);
}

#[test]
fn test_set_and_read_price() {
    let fixture = MockOracleTestFixture::new();

    set_price_with_auth(
        &fixture,
        &fixture.admin,
        &fixture.asset,
        2_345_678_900,
        1_700_000_100,
    );

    assert_eq!(
        fixture.client.lastprice(&fixture.asset),
        (2_345_678_900, 1_700_000_100)
    );
}

#[test]
fn test_set_and_read_decimals() {
    let fixture = MockOracleTestFixture::new();

    assert_eq!(fixture.client.decimals(&fixture.asset), DEFAULT_DECIMALS);

    set_decimals_with_auth(&fixture, &fixture.admin, &fixture.asset, 9);

    assert_eq!(fixture.client.decimals(&fixture.asset), 9);
}

#[test]
#[should_panic]
fn test_set_price_requires_admin() {
    let fixture = MockOracleTestFixture::new();

    set_price_with_auth(
        &fixture,
        &fixture.non_admin,
        &fixture.asset,
        1_500_000_000,
        1_700_000_050,
    );
}
