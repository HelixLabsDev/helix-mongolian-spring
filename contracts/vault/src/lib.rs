#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    token::TokenClient, Address, BytesN, Env, Symbol, TryFromVal, Val, Vec,
};

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;
const ADMIN_ROLE: u32 = 0;
const LIQUIDATOR_ROLE: u32 = 1;
const PAUSER_ROLE: u32 = 2;
const BPS_DENOMINATOR: i128 = 10_000;
const PRICE_SCALAR: i128 = 10_000_000;
const ORACLE_DECIMALS: u32 = 7;
const ORACLE_STALE_WINDOW: u64 = 3_600;
const SECONDS_PER_YEAR: i128 = 365 * 24 * 3_600;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PoolConfig,
    SupportedAssets,
    OracleAddress,
    BorrowToken,
    Paused,
    Locked,
    TotalDeposits,
    TotalBorrows,
    Position(Address),
    Role(Address, u32),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub deposited_shares: i128,
    pub borrowed_amount: i128,
    pub last_update: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolConfig {
    pub max_ltv: u32,
    pub liq_threshold: u32,
    pub liq_bonus: u32,
    pub interest_rate: u32,
    pub min_position: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InsufficientCollateral = 3,
    InsufficientLiquidity = 4,
    InvalidAmount = 5,
    Unauthorized = 6,
    PositionNotFound = 7,
    OracleStale = 8,
    Paused = 9,
    Reentrancy = 10,
    HealthFactorTooLow = 11,
    ClawbackDetected = 12,
    BelowMinPosition = 13,
}

#[contractclient(name = "CollateralTokenClient")]
pub trait CollateralTokenInterface {
    fn balance(env: Env, id: Address) -> i128;
    fn transfer(env: Env, from: Address, to: Address, amount: i128);
    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128);
    fn assets_for_shares(env: Env, shares: i128) -> i128;
    fn shares_for_assets(env: Env, assets: i128) -> i128;
}

#[contractclient(name = "OracleClient")]
pub trait OracleInterface {
    fn lastprice(env: Env, asset: Address) -> (i128, u64);
    fn decimals(env: Env, asset: Address) -> u32;
}

#[contract]
pub struct HelixVault;

#[contractimpl]
impl HelixVault {
    pub fn initialize(
        env: Env,
        admin: Address,
        oracle: Address,
        borrow_token: Address,
        config: PoolConfig,
    ) {
        Self::assert_not_initialized(&env);
        admin.require_auth();
        Self::validate_config(&env, &config);

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PoolConfig, &config);
        env.storage()
            .instance()
            .set(&DataKey::SupportedAssets, &Vec::<Address>::new(&env));
        env.storage().instance().set(&DataKey::OracleAddress, &oracle);
        env.storage()
            .instance()
            .set(&DataKey::BorrowToken, &borrow_token);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::Locked, &false);
        env.storage().instance().set(&DataKey::TotalDeposits, &0_i128);
        env.storage().instance().set(&DataKey::TotalBorrows, &0_i128);
        Self::write_role(&env, &admin, ADMIN_ROLE, true);
        Self::extend_instance(&env);
    }

    pub fn deposit(env: Env, user: Address, collateral_token: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_positive_amount(&env, amount);
        user.require_auth();
        Self::require_not_paused(&env);
        Self::require_supported_asset(&env, &collateral_token);

        let current_contract = env.current_contract_address();
        Self::with_lock(&env, || {
            let collateral_client = CollateralTokenClient::new(&env, &collateral_token);
            let mut position = Self::read_position(&env, &user).unwrap_or(Position {
                deposited_shares: 0,
                borrowed_amount: 0,
                last_update: env.ledger().timestamp(),
            });

            if position.borrowed_amount > 0 {
                position = Self::accrue_position(&env, position);
            } else {
                position.last_update = env.ledger().timestamp();
            }

            let vault_balance_before = collateral_client.balance(&current_contract);
            collateral_client.transfer_from(&current_contract, &user, &current_contract, &amount);
            let vault_balance_after = collateral_client.balance(&current_contract);
            Self::require_exact_incoming_delta(
                &env,
                vault_balance_before,
                vault_balance_after,
                amount,
            );

            position.deposited_shares =
                Self::checked_add(&env, position.deposited_shares, amount);
            Self::enforce_min_position(&env, position.deposited_shares);

            let total_deposits = Self::read_total_deposits(&env);
            let updated_total_deposits = Self::checked_add(&env, total_deposits, amount);

            Self::write_total_deposits(&env, updated_total_deposits);
            Self::write_position(&env, &user, &position);
            env.events().publish(
                (Symbol::new(&env, "deposit"), user),
                (collateral_token, amount, position),
            );
        });
    }

    pub fn withdraw(env: Env, user: Address, collateral_token: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_positive_amount(&env, amount);
        user.require_auth();
        Self::require_not_paused(&env);
        Self::require_supported_asset(&env, &collateral_token);

        let current_contract = env.current_contract_address();
        Self::with_lock(&env, || {
            let collateral_client = CollateralTokenClient::new(&env, &collateral_token);
            let mut position = Self::read_position_required(&env, &user);
            position = Self::accrue_position(&env, position);

            if position.deposited_shares < amount {
                panic_with_error!(&env, VaultError::InsufficientCollateral);
            }

            position.deposited_shares = Self::checked_sub(&env, position.deposited_shares, amount);
            Self::enforce_min_position(&env, position.deposited_shares);

            if position.borrowed_amount > 0 {
                let health_factor =
                    Self::health_factor_for_position(&env, &collateral_token, &position);
                if health_factor <= BPS_DENOMINATOR {
                    panic_with_error!(&env, VaultError::HealthFactorTooLow);
                }
            }

            let vault_balance_before = collateral_client.balance(&current_contract);
            collateral_client.transfer(&current_contract, &user, &amount);
            let vault_balance_after = collateral_client.balance(&current_contract);
            Self::require_exact_outgoing_delta(
                &env,
                vault_balance_before,
                vault_balance_after,
                amount,
            );

            let total_deposits = Self::read_total_deposits(&env);
            let updated_total_deposits = Self::checked_sub(&env, total_deposits, amount);

            Self::write_total_deposits(&env, updated_total_deposits);
            Self::store_or_remove_position(&env, &user, &position);
            env.events().publish(
                (Symbol::new(&env, "withdraw"), user),
                (collateral_token, amount, position),
            );
        });
    }

    pub fn borrow(env: Env, user: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_positive_amount(&env, amount);
        user.require_auth();
        Self::require_not_paused(&env);

        let borrow_token = Self::borrow_token(&env);
        let current_contract = env.current_contract_address();
        Self::with_lock(&env, || {
            let borrow_client = TokenClient::new(&env, &borrow_token);
            let collateral_token = Self::primary_supported_asset(&env);
            let mut position = Self::read_position(&env, &user).unwrap_or(Position {
                deposited_shares: 0,
                borrowed_amount: 0,
                last_update: env.ledger().timestamp(),
            });
            position = Self::accrue_position(&env, position);
            position.borrowed_amount = Self::checked_add(&env, position.borrowed_amount, amount);

            let max_borrow_allowed =
                Self::max_borrow_for_position(&env, &collateral_token, &position);
            if position.borrowed_amount > max_borrow_allowed {
                panic_with_error!(&env, VaultError::InsufficientCollateral);
            }

            let vault_balance_before = borrow_client.balance(&current_contract);
            if vault_balance_before < amount {
                panic_with_error!(&env, VaultError::InsufficientLiquidity);
            }

            borrow_client.transfer(&current_contract, &user, &amount);
            let vault_balance_after = borrow_client.balance(&current_contract);
            Self::require_exact_outgoing_delta(
                &env,
                vault_balance_before,
                vault_balance_after,
                amount,
            );

            let total_borrows = Self::read_total_borrows(&env);
            let updated_total_borrows = Self::checked_add(&env, total_borrows, amount);
            let health_factor =
                Self::health_factor_for_position(&env, &collateral_token, &position);

            Self::write_total_borrows(&env, updated_total_borrows);
            Self::write_position(&env, &user, &position);
            env.events().publish(
                (Symbol::new(&env, "borrow"), user),
                (amount, health_factor),
            );
        });
    }

    pub fn repay(env: Env, user: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_positive_amount(&env, amount);
        user.require_auth();

        let borrow_token = Self::borrow_token(&env);
        let current_contract = env.current_contract_address();
        Self::with_lock(&env, || {
            let borrow_client = TokenClient::new(&env, &borrow_token);
            let mut position = Self::read_position_required(&env, &user);
            position = Self::accrue_position(&env, position);

            let actual_repay = if amount > position.borrowed_amount {
                position.borrowed_amount
            } else {
                amount
            };

            if actual_repay > 0 {
                let vault_balance_before = borrow_client.balance(&current_contract);
                borrow_client.transfer_from(
                    &current_contract,
                    &user,
                    &current_contract,
                    &actual_repay,
                );
                let vault_balance_after = borrow_client.balance(&current_contract);
                Self::require_exact_incoming_delta(
                    &env,
                    vault_balance_before,
                    vault_balance_after,
                    actual_repay,
                );
            }

            position.borrowed_amount = Self::checked_sub(&env, position.borrowed_amount, actual_repay);

            let total_borrows = Self::read_total_borrows(&env);
            let updated_total_borrows = Self::checked_sub(&env, total_borrows, actual_repay);

            Self::write_total_borrows(&env, updated_total_borrows);
            Self::store_or_remove_position(&env, &user, &position);
            env.events().publish(
                (Symbol::new(&env, "repay"), user),
                (actual_repay, position.borrowed_amount),
            );
        });
    }

    pub fn liquidate(env: Env, liquidator: Address, user: Address, repay_amount: i128) {
        Self::require_initialized(&env);
        Self::validate_positive_amount(&env, repay_amount);
        liquidator.require_auth();
        Self::require_role(&env, &liquidator, LIQUIDATOR_ROLE);
        Self::require_not_paused(&env);

        let collateral_token = Self::primary_supported_asset(&env);
        let borrow_token = Self::borrow_token(&env);
        let current_contract = env.current_contract_address();
        Self::with_lock(&env, || {
            let collateral_client = CollateralTokenClient::new(&env, &collateral_token);
            let borrow_client = TokenClient::new(&env, &borrow_token);
            let config = Self::pool_config(&env);
            let mut position = Self::read_position_required(&env, &user);
            position = Self::accrue_position(&env, position);

            let health_factor = Self::health_factor_for_position(&env, &collateral_token, &position);
            if health_factor > BPS_DENOMINATOR {
                panic_with_error!(&env, VaultError::HealthFactorTooLow);
            }

            let repay_cap = position.borrowed_amount / 2;
            let actual_repay = if repay_amount > repay_cap {
                repay_cap
            } else {
                repay_amount
            };
            Self::validate_positive_amount(&env, actual_repay);

            let borrow_balance_before = borrow_client.balance(&current_contract);
            borrow_client.transfer_from(
                &current_contract,
                &liquidator,
                &current_contract,
                &actual_repay,
            );
            let borrow_balance_after = borrow_client.balance(&current_contract);
            Self::require_exact_incoming_delta(
                &env,
                borrow_balance_before,
                borrow_balance_after,
                actual_repay,
            );

            let bonus = Self::checked_mul_div_floor(
                &env,
                actual_repay,
                i128::from(config.liq_bonus),
                BPS_DENOMINATOR,
            );
            let seize_value = Self::checked_add(&env, actual_repay, bonus);
            let price = Self::oracle_price(&env, &collateral_token);
            let assets_to_seize =
                Self::checked_mul_div_floor(&env, seize_value, PRICE_SCALAR, price);
            let shares_to_seize = collateral_client.shares_for_assets(&assets_to_seize);
            Self::validate_positive_amount(&env, shares_to_seize);

            if position.deposited_shares < shares_to_seize {
                panic_with_error!(&env, VaultError::InsufficientCollateral);
            }

            let collateral_balance_before = collateral_client.balance(&current_contract);
            collateral_client.transfer(&current_contract, &liquidator, &shares_to_seize);
            let collateral_balance_after = collateral_client.balance(&current_contract);
            Self::require_exact_outgoing_delta(
                &env,
                collateral_balance_before,
                collateral_balance_after,
                shares_to_seize,
            );

            position.deposited_shares =
                Self::checked_sub(&env, position.deposited_shares, shares_to_seize);
            position.borrowed_amount =
                Self::checked_sub(&env, position.borrowed_amount, actual_repay);

            let total_deposits = Self::read_total_deposits(&env);
            let total_borrows = Self::read_total_borrows(&env);
            let updated_total_deposits =
                Self::checked_sub(&env, total_deposits, shares_to_seize);
            let updated_total_borrows = Self::checked_sub(&env, total_borrows, actual_repay);

            Self::write_total_deposits(&env, updated_total_deposits);
            Self::write_total_borrows(&env, updated_total_borrows);
            Self::store_or_remove_position(&env, &user, &position);
            env.events().publish(
                (Symbol::new(&env, "liquidation"), liquidator, user),
                (actual_repay, shares_to_seize, bonus),
            );
        });
    }

    pub fn get_health_factor(env: Env, user: Address) -> i128 {
        Self::require_initialized(&env);
        let collateral_token = Self::primary_supported_asset(&env);
        let position = Self::read_position_required(&env, &user);
        let preview = Self::preview_accrued_position(&env, &position);
        Self::health_factor_for_position(&env, &collateral_token, &preview)
    }

    pub fn get_position(env: Env, user: Address) -> Position {
        Self::require_initialized(&env);
        Self::read_position_required(&env, &user)
    }

    pub fn pause(env: Env) {
        Self::require_initialized(&env);
        let caller = Self::require_pauser_auth(&env);
        Self::set_paused(&env, true);
        Self::extend_instance(&env);
        env.events().publish((Symbol::new(&env, "pause"), caller), ());
    }

    pub fn unpause(env: Env) {
        Self::require_initialized(&env);
        let caller = Self::require_pauser_auth(&env);
        Self::set_paused(&env, false);
        Self::extend_instance(&env);
        env.events().publish((Symbol::new(&env, "unpause"), caller), ());
    }

    pub fn update_config(env: Env, new_config: PoolConfig) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        Self::validate_config(&env, &new_config);
        env.storage().instance().set(&DataKey::PoolConfig, &new_config);
        Self::extend_instance(&env);
        env.events()
            .publish((Symbol::new(&env, "config_update"),), new_config);
    }

    pub fn add_supported_asset(env: Env, asset: Address) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        let mut assets = Self::supported_assets(&env);
        if !Self::contains_address(&assets, &asset) {
            assets.push_back(asset);
            env.storage().instance().set(&DataKey::SupportedAssets, &assets);
        }
        Self::extend_instance(&env);
    }

    pub fn remove_supported_asset(env: Env, asset: Address) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        let assets = Self::supported_assets(&env);
        let mut filtered = Vec::new(&env);
        for supported in assets.iter() {
            if supported != asset {
                filtered.push_back(supported);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::SupportedAssets, &filtered);
        Self::extend_instance(&env);
    }

    pub fn grant_role(env: Env, addr: Address, role: u32) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        Self::write_role(&env, &addr, role, true);
        Self::extend_instance(&env);
    }

    pub fn revoke_role(env: Env, addr: Address, role: u32) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        Self::write_role(&env, &addr, role, false);
        Self::extend_instance(&env);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        Self::extend_instance(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

impl HelixVault {
    fn assert_not_initialized(env: &Env) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, VaultError::AlreadyInitialized);
        }
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, VaultError::NotInitialized);
        }
    }

    fn extend_instance(env: &Env) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
    }

    fn get_instance<T>(env: &Env, key: &DataKey) -> T
    where
        T: TryFromVal<Env, Val>,
    {
        match env.storage().instance().get::<_, T>(key) {
            Some(value) => value,
            None => panic_with_error!(env, VaultError::NotInitialized),
        }
    }

    fn admin(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Admin)
    }

    fn pool_config(env: &Env) -> PoolConfig {
        Self::get_instance(env, &DataKey::PoolConfig)
    }

    fn supported_assets(env: &Env) -> Vec<Address> {
        Self::get_instance(env, &DataKey::SupportedAssets)
    }

    fn borrow_token(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::BorrowToken)
    }

    fn oracle_address(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::OracleAddress)
    }

    fn primary_supported_asset(env: &Env) -> Address {
        let assets = Self::supported_assets(env);
        match assets.first() {
            Some(asset) => asset,
            None => panic_with_error!(env, VaultError::InvalidAmount),
        }
    }

    fn paused(env: &Env) -> bool {
        Self::get_instance(env, &DataKey::Paused)
    }

    fn set_paused(env: &Env, paused: bool) {
        env.storage().instance().set(&DataKey::Paused, &paused);
    }

    fn locked(env: &Env) -> bool {
        Self::get_instance(env, &DataKey::Locked)
    }

    fn set_locked(env: &Env, locked: bool) {
        env.storage().instance().set(&DataKey::Locked, &locked);
    }

    fn read_total_deposits(env: &Env) -> i128 {
        Self::get_instance(env, &DataKey::TotalDeposits)
    }

    fn write_total_deposits(env: &Env, total_deposits: i128) {
        env.storage()
            .instance()
            .set(&DataKey::TotalDeposits, &total_deposits);
    }

    fn read_total_borrows(env: &Env) -> i128 {
        Self::get_instance(env, &DataKey::TotalBorrows)
    }

    fn write_total_borrows(env: &Env, total_borrows: i128) {
        env.storage()
            .instance()
            .set(&DataKey::TotalBorrows, &total_borrows);
    }

    fn position_key(user: &Address) -> DataKey {
        DataKey::Position(user.clone())
    }

    fn role_key(addr: &Address, role: u32) -> DataKey {
        DataKey::Role(addr.clone(), role)
    }

    fn read_position(env: &Env, user: &Address) -> Option<Position> {
        let key = Self::position_key(user);
        env.storage().persistent().get(&key)
    }

    fn read_position_required(env: &Env, user: &Address) -> Position {
        match Self::read_position(env, user) {
            Some(position) => position,
            None => panic_with_error!(env, VaultError::PositionNotFound),
        }
    }

    fn write_position(env: &Env, user: &Address, position: &Position) {
        let key = Self::position_key(user);
        env.storage().persistent().set(&key, position);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn store_or_remove_position(env: &Env, user: &Address, position: &Position) {
        let key = Self::position_key(user);
        if position.deposited_shares == 0 && position.borrowed_amount == 0 {
            env.storage().persistent().remove(&key);
        } else {
            Self::write_position(env, user, position);
        }
    }

    fn has_role(env: &Env, addr: &Address, role: u32) -> bool {
        let key = Self::role_key(addr, role);
        env.storage().persistent().get(&key).unwrap_or(false)
    }

    fn require_role(env: &Env, addr: &Address, role: u32) {
        if !Self::has_role(env, addr, role) {
            panic_with_error!(env, VaultError::Unauthorized);
        }
    }

    fn write_role(env: &Env, addr: &Address, role: u32, enabled: bool) {
        let key = Self::role_key(addr, role);
        if enabled {
            env.storage().persistent().set(&key, &true);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
        } else {
            env.storage().persistent().remove(&key);
        }
    }

    fn require_admin_auth(env: &Env) -> Address {
        let admin = Self::admin(env);
        admin.require_auth();
        admin
    }

    fn require_pauser_auth(env: &Env) -> Address {
        let admin = Self::admin(env);
        admin.require_auth();
        if !Self::has_role(env, &admin, PAUSER_ROLE) {
            panic_with_error!(env, VaultError::Unauthorized);
        }
        admin
    }

    fn require_not_paused(env: &Env) {
        if Self::paused(env) {
            panic_with_error!(env, VaultError::Paused);
        }
    }

    fn require_supported_asset(env: &Env, asset: &Address) {
        if !Self::contains_address(&Self::supported_assets(env), asset) {
            panic_with_error!(env, VaultError::InvalidAmount);
        }
    }

    fn contains_address(addresses: &Vec<Address>, target: &Address) -> bool {
        for address in addresses.iter() {
            if address == *target {
                return true;
            }
        }
        false
    }

    fn with_lock<T, F>(env: &Env, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        if Self::locked(env) {
            panic_with_error!(env, VaultError::Reentrancy);
        }

        Self::set_locked(env, true);
        Self::extend_instance(env);
        let result = f();
        Self::set_locked(env, false);
        Self::extend_instance(env);
        result
    }

    fn preview_accrued_position(env: &Env, position: &Position) -> Position {
        let mut updated = position.clone();
        let elapsed_seconds = env.ledger().timestamp().saturating_sub(position.last_update);
        if updated.borrowed_amount > 0 && elapsed_seconds > 0 {
            let config = Self::pool_config(env);
            let accrued = Self::checked_mul_div_floor(
                env,
                Self::checked_mul(env, updated.borrowed_amount, i128::from(config.interest_rate)),
                i128::from(elapsed_seconds),
                Self::checked_mul(env, SECONDS_PER_YEAR, BPS_DENOMINATOR),
            );
            updated.borrowed_amount = Self::checked_add(env, updated.borrowed_amount, accrued);
        }
        updated.last_update = env.ledger().timestamp();
        updated
    }

    fn accrue_position(env: &Env, position: Position) -> Position {
        Self::preview_accrued_position(env, &position)
    }

    fn max_borrow_for_position(env: &Env, collateral_token: &Address, position: &Position) -> i128 {
        let config = Self::pool_config(env);
        let collateral_value =
            Self::collateral_value_for_position(env, collateral_token, position.deposited_shares);
        Self::checked_mul_div_floor(
            env,
            collateral_value,
            i128::from(config.max_ltv),
            BPS_DENOMINATOR,
        )
    }

    fn health_factor_for_position(
        env: &Env,
        collateral_token: &Address,
        position: &Position,
    ) -> i128 {
        if position.borrowed_amount == 0 {
            return i128::MAX;
        }

        let config = Self::pool_config(env);
        let collateral_value =
            Self::collateral_value_for_position(env, collateral_token, position.deposited_shares);
        Self::checked_mul_div_floor(
            env,
            collateral_value,
            i128::from(config.liq_threshold),
            position.borrowed_amount,
        )
    }

    fn collateral_value_for_position(
        env: &Env,
        collateral_token: &Address,
        deposited_shares: i128,
    ) -> i128 {
        if deposited_shares == 0 {
            return 0;
        }

        let collateral_client = CollateralTokenClient::new(env, collateral_token);
        let assets = collateral_client.assets_for_shares(&deposited_shares);
        let price = Self::oracle_price(env, collateral_token);
        Self::checked_mul_div_floor(env, assets, price, PRICE_SCALAR)
    }

    fn oracle_price(env: &Env, asset: &Address) -> i128 {
        let oracle = OracleClient::new(env, &Self::oracle_address(env));
        let decimals = oracle.decimals(asset);
        if decimals != ORACLE_DECIMALS {
            panic_with_error!(env, VaultError::InvalidAmount);
        }

        let (price, timestamp) = oracle.lastprice(asset);
        if price <= 0 {
            panic_with_error!(env, VaultError::InvalidAmount);
        }

        if env.ledger().timestamp().saturating_sub(timestamp) > ORACLE_STALE_WINDOW {
            panic_with_error!(env, VaultError::OracleStale);
        }

        price
    }

    fn validate_config(env: &Env, config: &PoolConfig) {
        if config.min_position <= 0
            || config.max_ltv == 0
            || config.max_ltv > 10_000
            || config.liq_threshold == 0
            || config.liq_threshold > 10_000
            || config.liq_bonus > 10_000
        {
            panic_with_error!(env, VaultError::InvalidAmount);
        }
    }

    fn enforce_min_position(env: &Env, deposited_shares: i128) {
        if deposited_shares == 0 {
            return;
        }

        let config = Self::pool_config(env);
        if deposited_shares < config.min_position {
            panic_with_error!(env, VaultError::BelowMinPosition);
        }
    }

    fn require_exact_incoming_delta(
        env: &Env,
        balance_before: i128,
        balance_after: i128,
        expected_delta: i128,
    ) {
        let actual_delta = Self::checked_sub(env, balance_after, balance_before);
        if actual_delta != expected_delta {
            panic_with_error!(env, VaultError::ClawbackDetected);
        }
    }

    fn require_exact_outgoing_delta(
        env: &Env,
        balance_before: i128,
        balance_after: i128,
        expected_delta: i128,
    ) {
        let actual_delta = Self::checked_sub(env, balance_before, balance_after);
        if actual_delta != expected_delta {
            panic_with_error!(env, VaultError::ClawbackDetected);
        }
    }

    fn validate_positive_amount(env: &Env, amount: i128) {
        if amount <= 0 {
            panic_with_error!(env, VaultError::InvalidAmount);
        }
    }

    fn checked_add(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_add(right) {
            Some(value) => value,
            None => panic_with_error!(env, VaultError::InvalidAmount),
        }
    }

    fn checked_sub(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_sub(right) {
            Some(value) => value,
            None => panic_with_error!(env, VaultError::InvalidAmount),
        }
    }

    fn checked_mul(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_mul(right) {
            Some(value) => value,
            None => panic_with_error!(env, VaultError::InvalidAmount),
        }
    }

    fn checked_mul_div_floor(env: &Env, left: i128, right: i128, denominator: i128) -> i128 {
        if denominator <= 0 {
            panic_with_error!(env, VaultError::InvalidAmount);
        }

        let product = Self::checked_mul(env, left, right);
        product / denominator
    }
}

#[cfg(test)]
mod test;
