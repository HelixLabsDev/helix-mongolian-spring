#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};

use soroban_sdk::{
    testutils::{
        storage::Instance as _,
        Address as _, Events as _, MockAuth, MockAuthInvoke,
    },
    vec, Address, Env, IntoVal, String, Symbol, Val,
};

use super::{AllowanceData, DataKey, HelixToken, HelixTokenClient};

const DECIMALS: u32 = 7;

struct TokenTestFixture<'a> {
    env: Env,
    client: HelixTokenClient<'a>,
    admin: Address,
    vault: Address,
    bridge: Address,
    user1: Address,
    user2: Address,
}

impl<'a> TokenTestFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        let contract_id = env.register(HelixToken, ());
        let client = HelixTokenClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let bridge = Address::generate(&env);
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(
            &admin,
            &vault,
            &bridge,
            &token_name(&env),
            &token_symbol(&env),
            &DECIMALS,
        );
        env.set_auths(&[]);

        Self {
            env,
            client,
            admin,
            vault,
            bridge,
            user1,
            user2,
        }
    }
}

fn token_name(env: &Env) -> String {
    String::from_str(env, "Helix Staked ETH")
}

fn token_symbol(env: &Env) -> String {
    String::from_str(env, "hstETH")
}

fn instance_value<T>(env: &Env, contract: &Address, key: &DataKey) -> T
where
    T: soroban_sdk::TryFromVal<Env, Val>,
{
    env.as_contract(contract, || {
        env.storage()
            .instance()
            .get::<_, T>(key)
            .expect("instance value must exist")
    })
}

fn balance_value(env: &Env, contract: &Address, address: &Address) -> i128 {
    env.as_contract(contract, || {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::Balance(address.clone()))
            .unwrap_or(0)
    })
}

fn allowance_value(
    env: &Env,
    contract: &Address,
    from: &Address,
    spender: &Address,
) -> Option<AllowanceData> {
    env.as_contract(contract, || {
        env.storage()
            .temporary()
            .get::<_, AllowanceData>(&DataKey::Allowance(from.clone(), spender.clone()))
    })
}

fn set_admin_with_auth(fixture: &TokenTestFixture<'_>, signer: &Address, new_admin: &Address) {
    let invocation = MockAuthInvoke {
        contract: &fixture.client.address,
        fn_name: "set_admin",
        args: (new_admin.clone(),).into_val(&fixture.env),
        sub_invokes: &[],
    };
    let auth = MockAuth {
        address: signer,
        invoke: &invocation,
    };

    fixture.client.mock_auths(&[auth]).set_admin(new_admin);
}

#[test]
fn test_initialize() {
    let fixture = TokenTestFixture::new();

    assert_eq!(fixture.client.name(), token_name(&fixture.env));
    assert_eq!(fixture.client.symbol(), token_symbol(&fixture.env));
    assert_eq!(fixture.client.decimals(), DECIMALS);
    assert_eq!(fixture.client.total_supply(), 0);
    assert_eq!(fixture.client.exchange_rate(), (0, 0));

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Admin),
        fixture.admin
    );
    assert_eq!(
        instance_value::<Address>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::VaultContract
        ),
        fixture.vault
    );
    assert_eq!(
        instance_value::<Address>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::BridgeHandler
        ),
        fixture.bridge
    );
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.client.address, &DataKey::TotalShares),
        0
    );
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.client.address, &DataKey::TotalAssets),
        0
    );
    assert!(
        fixture
            .env
            .as_contract(&fixture.client.address, || fixture.env.storage().instance().all())
            .len()
            >= 8
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice_fails() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.initialize(
        &fixture.admin,
        &fixture.vault,
        &fixture.bridge,
        &token_name(&fixture.env),
        &token_symbol(&fixture.env),
        &DECIMALS,
    );
}

#[test]
fn test_vault_mint() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);

    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.client.address.clone(),
                (Symbol::new(&fixture.env, "mint"), fixture.user1.clone()).into_val(&fixture.env),
                (100_i128, 100_i128).into_val(&fixture.env)
            )
        ]
    );

    assert_eq!(fixture.client.balance(&fixture.user1), 100);
    assert_eq!(fixture.client.total_supply(), 100);
    assert_eq!(balance_value(&fixture.env, &fixture.client.address, &fixture.user1), 100);
}

#[test]
#[should_panic]
fn test_vault_mint_unauthorized() {
    let fixture = TokenTestFixture::new();
    fixture.client.vault_mint(&fixture.user1, &100);
}

#[test]
fn test_vault_burn() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);
    fixture.client.vault_burn(&fixture.user1, &40);

    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.client.address.clone(),
                (Symbol::new(&fixture.env, "burn"), fixture.user1.clone()).into_val(&fixture.env),
                (40_i128, 60_i128).into_val(&fixture.env)
            )
        ]
    );

    assert_eq!(fixture.client.balance(&fixture.user1), 60);
    assert_eq!(fixture.client.total_supply(), 60);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_vault_burn_insufficient() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &10);
    fixture.client.vault_burn(&fixture.user1, &20);
}

#[test]
fn test_exchange_rate_update() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);
    fixture.client.update_exchange_rate(&250);

    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];
    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.client.address.clone(),
                (Symbol::new(&fixture.env, "exchange_rate_update"),).into_val(&fixture.env),
                (0_i128, 250_i128, 100_i128).into_val(&fixture.env)
            )
        ]
    );

    assert_eq!(fixture.client.exchange_rate(), (250, 100));
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.client.address, &DataKey::TotalAssets),
        250
    );
}

#[test]
#[should_panic]
fn test_exchange_rate_update_unauthorized() {
    let fixture = TokenTestFixture::new();
    fixture.client.update_exchange_rate(&100);
}

#[test]
fn test_assets_for_shares() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);
    fixture.client.update_exchange_rate(&250);

    assert_eq!(fixture.client.assets_for_shares(&40), 100);
    assert_eq!(fixture.client.assets_for_shares(&0), 0);
}

#[test]
fn test_shares_for_assets() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);
    fixture.client.update_exchange_rate(&250);

    assert_eq!(fixture.client.shares_for_assets(&125), 50);
    assert_eq!(fixture.client.shares_for_assets(&0), 0);
}

#[test]
fn test_transfer() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);
    fixture.client.transfer(&fixture.user1, &fixture.user2, &35);

    assert_eq!(fixture.client.balance(&fixture.user1), 65);
    assert_eq!(fixture.client.balance(&fixture.user2), 35);
}

#[test]
fn test_approve_and_transfer_from() {
    let fixture = TokenTestFixture::new();
    fixture.env.mock_all_auths();

    fixture.client.vault_mint(&fixture.user1, &100);

    let expiration = fixture.env.ledger().sequence() + 100;
    fixture
        .client
        .approve(&fixture.user1, &fixture.user2, &60, &expiration);
    fixture
        .client
        .transfer_from(&fixture.user2, &fixture.user1, &fixture.user2, &25);

    assert_eq!(fixture.client.balance(&fixture.user1), 75);
    assert_eq!(fixture.client.balance(&fixture.user2), 25);
    assert_eq!(fixture.client.allowance(&fixture.user1, &fixture.user2), 35);
    assert_eq!(
        allowance_value(
            &fixture.env,
            &fixture.client.address,
            &fixture.user1,
            &fixture.user2
        ),
        Some(AllowanceData {
            amount: 35,
            expiration_ledger: expiration,
        })
    );
}

#[test]
fn test_set_admin() {
    let fixture = TokenTestFixture::new();
    let new_admin = Address::generate(&fixture.env);
    let final_admin = Address::generate(&fixture.env);

    set_admin_with_auth(&fixture, &fixture.admin, &new_admin);

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Admin),
        new_admin
    );

    let old_admin_retry = catch_unwind(AssertUnwindSafe(|| {
        set_admin_with_auth(&fixture, &fixture.admin, &final_admin);
    }));
    assert!(old_admin_retry.is_err());

    set_admin_with_auth(&fixture, &new_admin, &final_admin);

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Admin),
        final_admin
    );
}
