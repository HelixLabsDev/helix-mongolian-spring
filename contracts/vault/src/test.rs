#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Events as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env, IntoVal, Symbol, Val, Vec,
};

use super::{
    DataKey, HelixVault, HelixVaultClient, PoolConfig, Position, ADMIN_ROLE, LIQUIDATOR_ROLE,
    ORACLE_DECIMALS, PAUSER_ROLE,
};

const ONE: i128 = 10_000_000;

mod helix_token {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/helix_token.wasm");
}

#[contracttype]
#[derive(Clone)]
enum OracleDataKey {
    Price(Address),
}

#[contract]
pub struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage()
            .instance()
            .set(&OracleDataKey::Price(asset), &price);
    }

    pub fn lastprice(env: Env, asset: Address) -> (i128, u64) {
        let price = env
            .storage()
            .instance()
            .get::<_, i128>(&OracleDataKey::Price(asset))
            .unwrap_or(0);
        (price, env.ledger().timestamp())
    }

    pub fn decimals(_env: Env, _asset: Address) -> u32 {
        ORACLE_DECIMALS
    }
}

struct VaultTestFixture<'a> {
    env: Env,
    vault: HelixVaultClient<'a>,
    token: helix_token::Client<'a>,
    oracle: MockOracleClient<'a>,
    borrow_token: TokenClient<'a>,
    borrow_admin: StellarAssetClient<'a>,
    admin: Address,
    user: Address,
    liquidator: Address,
}

impl<'a> VaultTestFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_700_000_000);
        env.ledger().set_sequence_number(100);

        let admin = Address::generate(&env);
        let bridge = Address::generate(&env);
        let user = Address::generate(&env);
        let liquidator = Address::generate(&env);

        let vault_id = env.register(HelixVault, ());
        let token_id = env.register(helix_token::WASM, ());
        let oracle_id = env.register(MockOracle, ());
        let borrow_asset = env.register_stellar_asset_contract_v2(admin.clone());
        let borrow_token_address = borrow_asset.address();

        let vault = HelixVaultClient::new(&env, &vault_id);
        let token = helix_token::Client::new(&env, &token_id);
        let oracle = MockOracleClient::new(&env, &oracle_id);
        let borrow_token = TokenClient::new(&env, &borrow_token_address);
        let borrow_admin = StellarAssetClient::new(&env, &borrow_token_address);

        env.mock_all_auths();
        vault.initialize(
            &admin,
            &oracle_id,
            &borrow_token_address,
            &default_config(),
        );
        token.initialize(
            &admin,
            &vault_id,
            &bridge,
            &token_name(&env),
            &token_symbol(&env),
            &ORACLE_DECIMALS,
        );
        env.set_auths(&[]);

        Self {
            env,
            vault,
            token,
            oracle,
            borrow_token,
            borrow_admin,
            admin,
            user,
            liquidator,
        }
    }

    fn with_all_auths<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.env.mock_all_auths();
        let result = f();
        self.env.set_auths(&[]);
        result
    }

    fn expiration_ledger(&self) -> u32 {
        self.env.ledger().sequence() + 500
    }

    fn enable_collateral(&self) {
        self.with_all_auths(|| self.vault.add_supported_asset(&self.token.address));
    }

    fn grant_role(&self, addr: &Address, role: u32) {
        self.with_all_auths(|| self.vault.grant_role(addr, &role));
    }

    fn set_oracle_price(&self, price: i128) {
        self.with_all_auths(|| self.oracle.set_price(&self.token.address, &price));
    }

    fn set_exchange_rate(&self, total_assets: i128) {
        self.with_all_auths(|| self.token.update_exchange_rate(&total_assets));
    }

    fn mint_collateral(&self, to: &Address, shares: i128) {
        self.with_all_auths(|| self.token.vault_mint(to, &shares));
    }

    fn approve_collateral(&self, owner: &Address, amount: i128) {
        let expiration = self.expiration_ledger();
        self.with_all_auths(|| {
            self.token
                .approve(owner, &self.vault.address, &amount, &expiration)
        });
    }

    fn mint_borrow_token(&self, to: &Address, amount: i128) {
        self.with_all_auths(|| self.borrow_admin.mint(to, &amount));
    }

    fn approve_borrow_token(&self, owner: &Address, amount: i128) {
        let expiration = self.expiration_ledger();
        self.with_all_auths(|| {
            self.borrow_token
                .approve(owner, &self.vault.address, &amount, &expiration)
        });
    }

    fn deposit(&self, user: &Address, amount: i128) {
        self.with_all_auths(|| self.vault.deposit(user, &self.token.address, &amount));
    }

    fn withdraw(&self, user: &Address, amount: i128) {
        self.with_all_auths(|| self.vault.withdraw(user, &self.token.address, &amount));
    }

    fn borrow(&self, user: &Address, amount: i128) {
        self.with_all_auths(|| self.vault.borrow(user, &amount));
    }

    fn repay(&self, user: &Address, amount: i128) {
        self.with_all_auths(|| self.vault.repay(user, &amount));
    }

    fn liquidate(&self, liquidator: &Address, user: &Address, amount: i128) {
        self.with_all_auths(|| self.vault.liquidate(liquidator, user, &amount));
    }

    fn pause(&self) {
        self.with_all_auths(|| self.vault.pause());
    }
}

fn default_config() -> PoolConfig {
    PoolConfig {
        max_ltv: 7_500,
        liq_threshold: 8_000,
        liq_bonus: 500,
        interest_rate: 300,
        min_position: 10 * ONE,
    }
}

fn token_name(env: &Env) -> soroban_sdk::String {
    soroban_sdk::String::from_str(env, "Helix Staked ETH")
}

fn token_symbol(env: &Env) -> soroban_sdk::String {
    soroban_sdk::String::from_str(env, "hstETH")
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

fn role_value(env: &Env, contract: &Address, addr: &Address, role: u32) -> bool {
    env.as_contract(contract, || {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::Role(addr.clone(), role))
            .unwrap_or(false)
    })
}

#[test]
fn test_initialize() {
    let fixture = VaultTestFixture::new();

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.vault.address, &DataKey::Admin),
        fixture.admin
    );
    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.vault.address, &DataKey::OracleAddress),
        fixture.oracle.address
    );
    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.vault.address, &DataKey::BorrowToken),
        fixture.borrow_token.address
    );
    assert_eq!(
        instance_value::<PoolConfig>(&fixture.env, &fixture.vault.address, &DataKey::PoolConfig),
        default_config()
    );
    assert_eq!(
        instance_value::<Vec<Address>>(
            &fixture.env,
            &fixture.vault.address,
            &DataKey::SupportedAssets
        ),
        Vec::<Address>::new(&fixture.env)
    );
    assert!(!instance_value::<bool>(
        &fixture.env,
        &fixture.vault.address,
        &DataKey::Paused
    ));
    assert!(!instance_value::<bool>(
        &fixture.env,
        &fixture.vault.address,
        &DataKey::Locked
    ));
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalDeposits),
        0
    );
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalBorrows),
        0
    );
    assert!(role_value(
        &fixture.env,
        &fixture.vault.address,
        &fixture.admin,
        ADMIN_ROLE
    ));
}

#[test]
fn test_deposit() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.approve_collateral(&fixture.user, 40 * ONE);
    fixture.env.mock_all_auths();
    fixture
        .vault
        .deposit(&fixture.user, &fixture.token.address, &(40 * ONE));
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];

    let position = fixture.vault.get_position(&fixture.user);
    assert_eq!(
        position,
        Position {
            deposited_shares: 40 * ONE,
            borrowed_amount: 0,
            last_update: fixture.env.ledger().timestamp(),
        }
    );
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalDeposits),
        40 * ONE
    );
    assert_eq!(fixture.token.balance(&fixture.vault.address), 40 * ONE);

    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.vault.address.clone(),
                (Symbol::new(&fixture.env, "deposit"), fixture.user.clone()).into_val(&fixture.env),
                (
                    fixture.token.address.clone(),
                    40 * ONE,
                    Position {
                        deposited_shares: 40 * ONE,
                        borrowed_amount: 0,
                        last_update: fixture.env.ledger().timestamp(),
                    }
                )
                    .into_val(&fixture.env)
            )
        ]
    );
    fixture.env.set_auths(&[]);
}

#[test]
#[should_panic]
fn test_deposit_unauthorized() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.approve_collateral(&fixture.user, 40 * ONE);

    fixture
        .vault
        .deposit(&fixture.user, &fixture.token.address, &(40 * ONE));
}

#[test]
fn test_withdraw() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.approve_collateral(&fixture.user, 40 * ONE);
    fixture.deposit(&fixture.user, 40 * ONE);
    fixture.env.mock_all_auths();
    fixture
        .vault
        .withdraw(&fixture.user, &fixture.token.address, &(15 * ONE));
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];

    let position = fixture.vault.get_position(&fixture.user);
    assert_eq!(position.deposited_shares, 25 * ONE);
    assert_eq!(position.borrowed_amount, 0);
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalDeposits),
        25 * ONE
    );
    assert_eq!(fixture.token.balance(&fixture.user), 75 * ONE);

    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.vault.address.clone(),
                (Symbol::new(&fixture.env, "withdraw"), fixture.user.clone()).into_val(&fixture.env),
                (
                    fixture.token.address.clone(),
                    15 * ONE,
                    position
                )
                    .into_val(&fixture.env)
            )
        ]
    );
    fixture.env.set_auths(&[]);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_withdraw_health_check() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 200 * ONE);
    fixture.borrow(&fixture.user, 100 * ONE);

    fixture.withdraw(&fixture.user, 40 * ONE);
}

#[test]
fn test_borrow() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 200 * ONE);
    fixture.env.mock_all_auths();
    fixture.vault.borrow(&fixture.user, &(100 * ONE));
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];

    let position = fixture.vault.get_position(&fixture.user);
    assert_eq!(position.deposited_shares, 100 * ONE);
    assert_eq!(position.borrowed_amount, 100 * ONE);
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalBorrows),
        100 * ONE
    );
    assert_eq!(fixture.borrow_token.balance(&fixture.user), 100 * ONE);

    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.vault.address.clone(),
                (Symbol::new(&fixture.env, "borrow"), fixture.user.clone()).into_val(&fixture.env),
                (100 * ONE, 16_000_i128).into_val(&fixture.env)
            )
        ]
    );
    fixture.env.set_auths(&[]);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_borrow_exceeds_ltv() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 500 * ONE);

    fixture.borrow(&fixture.user, 160 * ONE);
}

#[test]
fn test_repay() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 200 * ONE);
    fixture.borrow(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.user, 50 * ONE);
    fixture.approve_borrow_token(&fixture.user, 150 * ONE);
    fixture.env.mock_all_auths();
    fixture.vault.repay(&fixture.user, &(150 * ONE));
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];

    let position = fixture.vault.get_position(&fixture.user);
    assert_eq!(position.deposited_shares, 100 * ONE);
    assert_eq!(position.borrowed_amount, 0);
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalBorrows),
        0
    );
    assert_eq!(fixture.borrow_token.balance(&fixture.user), 50 * ONE);

    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.vault.address.clone(),
                (Symbol::new(&fixture.env, "repay"), fixture.user.clone()).into_val(&fixture.env),
                (100 * ONE, 0_i128).into_val(&fixture.env)
            )
        ]
    );
    fixture.env.set_auths(&[]);
}

#[test]
fn test_liquidation() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.grant_role(&fixture.liquidator, LIQUIDATOR_ROLE);
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 300 * ONE);
    fixture.borrow(&fixture.user, 140 * ONE);
    fixture.set_oracle_price(ONE);
    fixture.mint_borrow_token(&fixture.liquidator, 100 * ONE);
    fixture.approve_borrow_token(&fixture.liquidator, 100 * ONE);
    fixture.env.mock_all_auths();
    fixture
        .vault
        .liquidate(&fixture.liquidator, &fixture.user, &(100 * ONE));
    let event = vec![&fixture.env, fixture.env.events().all().last_unchecked()];

    let position = fixture.vault.get_position(&fixture.user);
    assert_eq!(position.deposited_shares, 265_000_000);
    assert_eq!(position.borrowed_amount, 70 * ONE);
    assert_eq!(fixture.token.balance(&fixture.liquidator), 735_000_000);
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalDeposits),
        265_000_000
    );
    assert_eq!(
        instance_value::<i128>(&fixture.env, &fixture.vault.address, &DataKey::TotalBorrows),
        70 * ONE
    );

    assert_eq!(
        event,
        vec![
            &fixture.env,
            (
                fixture.vault.address.clone(),
                (
                    Symbol::new(&fixture.env, "liquidation"),
                    fixture.liquidator.clone(),
                    fixture.user.clone()
                )
                    .into_val(&fixture.env),
                (70 * ONE, 735_000_000_i128, 35_000_000_i128).into_val(&fixture.env)
            )
        ]
    );
    fixture.env.set_auths(&[]);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_liquidation_healthy_position() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.grant_role(&fixture.liquidator, LIQUIDATOR_ROLE);
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 300 * ONE);
    fixture.borrow(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.liquidator, 100 * ONE);
    fixture.approve_borrow_token(&fixture.liquidator, 100 * ONE);

    fixture.liquidate(&fixture.liquidator, &fixture.user, 50 * ONE);
}

#[test]
fn test_pause_blocks_operations() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.grant_role(&fixture.admin, PAUSER_ROLE);
    fixture.grant_role(&fixture.liquidator, LIQUIDATOR_ROLE);
    fixture.mint_collateral(&fixture.user, 150 * ONE);
    fixture.approve_collateral(&fixture.user, 150 * ONE);
    fixture.set_exchange_rate(150 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 300 * ONE);
    fixture.borrow(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.user, 50 * ONE);
    fixture.approve_borrow_token(&fixture.user, 50 * ONE);

    fixture.pause();

    fixture.env.mock_all_auths();
    let deposit_result = catch_unwind(AssertUnwindSafe(|| {
        fixture
            .vault
            .deposit(&fixture.user, &fixture.token.address, &(10 * ONE));
    }));
    let withdraw_result = catch_unwind(AssertUnwindSafe(|| {
        fixture
            .vault
            .withdraw(&fixture.user, &fixture.token.address, &(10 * ONE));
    }));
    let borrow_result = catch_unwind(AssertUnwindSafe(|| {
        fixture.vault.borrow(&fixture.user, &(10 * ONE));
    }));
    let liquidate_result = catch_unwind(AssertUnwindSafe(|| {
        fixture
            .vault
            .liquidate(&fixture.liquidator, &fixture.user, &(10 * ONE));
    }));
    fixture.env.set_auths(&[]);

    assert!(deposit_result.is_err());
    assert!(withdraw_result.is_err());
    assert!(borrow_result.is_err());
    assert!(liquidate_result.is_err());

    fixture.repay(&fixture.user, 50 * ONE);
    assert_eq!(fixture.vault.get_position(&fixture.user).borrowed_amount, 50 * ONE);
}

#[test]
fn test_interest_accrual() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.set_oracle_price(2 * ONE);
    fixture.approve_collateral(&fixture.user, 100 * ONE);
    fixture.deposit(&fixture.user, 100 * ONE);
    fixture.mint_borrow_token(&fixture.vault.address, 300 * ONE);
    fixture.borrow(&fixture.user, 100 * ONE);

    fixture
        .env
        .ledger()
        .set_timestamp(fixture.env.ledger().timestamp() + 31_536_000);
    fixture.mint_borrow_token(&fixture.user, ONE);
    fixture.approve_borrow_token(&fixture.user, ONE);
    fixture.repay(&fixture.user, ONE);

    let position = fixture.vault.get_position(&fixture.user);
    assert_eq!(position.borrowed_amount, 102 * ONE);
}

#[test]
fn test_reentrancy_guard() {
    let fixture = VaultTestFixture::new();
    fixture.enable_collateral();
    fixture.mint_collateral(&fixture.user, 100 * ONE);
    fixture.set_exchange_rate(100 * ONE);
    fixture.approve_collateral(&fixture.user, 50 * ONE);

    assert!(!instance_value::<bool>(
        &fixture.env,
        &fixture.vault.address,
        &DataKey::Locked
    ));

    fixture.deposit(&fixture.user, 50 * ONE);
    assert!(!instance_value::<bool>(
        &fixture.env,
        &fixture.vault.address,
        &DataKey::Locked
    ));

    let result = catch_unwind(AssertUnwindSafe(|| {
        fixture.env.mock_all_auths();
        fixture
            .vault
            .withdraw(&fixture.user, &fixture.token.address, &(60 * ONE));
    }));
    fixture.env.set_auths(&[]);
    assert!(result.is_err());
    assert!(!instance_value::<bool>(
        &fixture.env,
        &fixture.vault.address,
        &DataKey::Locked
    ));
}
