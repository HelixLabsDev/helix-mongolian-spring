#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token::TokenInterface,
    Address, BytesN, Env, String, Symbol, TryFromVal, Val,
};
use soroban_token_sdk::TokenUtils;

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    VaultContract,
    BridgeHandler,
    Name,
    Symbol,
    Decimals,
    TotalShares,
    TotalAssets,
    Balance(Address),
    Allowance(Address, Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceData {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    InsufficientBalance = 4,
    InvalidAmount = 5,
    InsufficientAllowance = 6,
    AllowanceExpired = 7,
}

#[contract]
pub struct HelixToken;

#[contractimpl]
impl HelixToken {
    pub fn initialize(
        env: Env,
        admin: Address,
        vault: Address,
        bridge: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) {
        Self::assert_not_initialized(&env);
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::VaultContract, &vault);
        env.storage().instance().set(&DataKey::BridgeHandler, &bridge);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::TotalShares, &0_i128);
        env.storage().instance().set(&DataKey::TotalAssets, &0_i128);
        Self::extend_instance(&env);
    }

    pub fn vault_mint(env: Env, to: Address, shares: i128) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, shares);

        let vault = Self::vault_contract(&env);
        vault.require_auth();

        Self::extend_instance(&env);
        let balance = Self::read_balance(&env, &to);
        let total_shares = Self::read_total_shares(&env);
        let updated_balance = Self::checked_add(&env, balance, shares);
        let updated_total_shares = Self::checked_add(&env, total_shares, shares);

        Self::write_balance(&env, &to, updated_balance);
        Self::write_total_shares(&env, updated_total_shares);

        env.events().publish(
            (Symbol::new(&env, "mint"), to),
            (shares, updated_total_shares),
        );
    }

    pub fn vault_burn(env: Env, from: Address, shares: i128) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, shares);

        let vault = Self::vault_contract(&env);
        vault.require_auth();

        Self::extend_instance(&env);
        Self::spend_balance(&env, &from, shares);

        let total_shares = Self::read_total_shares(&env);
        let updated_total_shares = Self::checked_sub(&env, total_shares, shares);
        Self::write_total_shares(&env, updated_total_shares);

        env.events().publish(
            (Symbol::new(&env, "burn"), from),
            (shares, updated_total_shares),
        );
    }

    pub fn update_exchange_rate(env: Env, new_total_assets: i128) {
        Self::require_initialized(&env);
        if new_total_assets <= 0 {
            panic_with_error!(&env, TokenError::InvalidAmount);
        }

        let bridge = Self::bridge_handler(&env);
        bridge.require_auth();

        Self::extend_instance(&env);
        let old_total_assets = Self::read_total_assets(&env);
        let total_shares = Self::read_total_shares(&env);

        env.storage()
            .instance()
            .set(&DataKey::TotalAssets, &new_total_assets);

        env.events().publish(
            (Symbol::new(&env, "exchange_rate_update"),),
            (old_total_assets, new_total_assets, total_shares),
        );
    }

    pub fn exchange_rate(env: Env) -> (i128, i128) {
        Self::require_initialized(&env);
        let total_shares = Self::read_total_shares(&env);
        if total_shares == 0 {
            return (0, 0);
        }
        (Self::read_total_assets(&env), total_shares)
    }

    pub fn assets_for_shares(env: Env, shares: i128) -> i128 {
        Self::require_initialized(&env);
        Self::validate_amount(&env, shares);

        let total_shares = Self::read_total_shares(&env);
        if total_shares == 0 || shares == 0 {
            return 0;
        }

        let total_assets = Self::read_total_assets(&env);
        let product = Self::checked_mul(&env, shares, total_assets);
        product / total_shares
    }

    pub fn shares_for_assets(env: Env, assets: i128) -> i128 {
        Self::require_initialized(&env);
        Self::validate_amount(&env, assets);

        let total_assets = Self::read_total_assets(&env);
        if total_assets == 0 || assets == 0 {
            return 0;
        }

        let total_shares = Self::read_total_shares(&env);
        let product = Self::checked_mul(&env, assets, total_shares);
        product / total_assets
    }

    pub fn total_supply(env: Env) -> i128 {
        Self::require_initialized(&env);
        Self::read_total_shares(&env)
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        Self::require_initialized(&env);
        let current_admin = Self::admin(&env);
        current_admin.require_auth();

        Self::extend_instance(&env);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        TokenUtils::new(&env)
            .events()
            .set_admin(current_admin, new_admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::require_initialized(&env);
        let admin = Self::admin(&env);
        admin.require_auth();

        Self::extend_instance(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

#[contractimpl]
impl TokenInterface for HelixToken {
    fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        Self::require_initialized(&env);
        Self::allowance_amount(&env, &from, &spender)
    }

    fn approve(env: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, amount);
        from.require_auth();

        Self::extend_instance(&env);
        Self::write_allowance(&env, &from, &spender, amount, expiration_ledger);

        TokenUtils::new(&env)
            .events()
            .approve(from, spender, amount, expiration_ledger);
    }

    fn balance(env: Env, id: Address) -> i128 {
        Self::require_initialized(&env);
        Self::read_balance(&env, &id)
    }

    fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, amount);
        from.require_auth();

        Self::extend_instance(&env);
        Self::spend_balance(&env, &from, amount);

        let recipient_balance = Self::read_balance(&env, &to);
        let updated_balance = Self::checked_add(&env, recipient_balance, amount);
        Self::write_balance(&env, &to, updated_balance);

        TokenUtils::new(&env).events().transfer(from, to, amount);
    }

    fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, amount);
        spender.require_auth();

        Self::extend_instance(&env);
        Self::spend_allowance(&env, &from, &spender, amount);
        Self::spend_balance(&env, &from, amount);

        let recipient_balance = Self::read_balance(&env, &to);
        let updated_balance = Self::checked_add(&env, recipient_balance, amount);
        Self::write_balance(&env, &to, updated_balance);

        TokenUtils::new(&env).events().transfer(from, to, amount);
    }

    fn burn(env: Env, from: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, amount);
        from.require_auth();

        Self::extend_instance(&env);
        Self::spend_balance(&env, &from, amount);

        let total_shares = Self::read_total_shares(&env);
        let updated_total_shares = Self::checked_sub(&env, total_shares, amount);
        Self::write_total_shares(&env, updated_total_shares);

        TokenUtils::new(&env).events().burn(from, amount);
    }

    fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        Self::require_initialized(&env);
        Self::validate_amount(&env, amount);
        spender.require_auth();

        Self::extend_instance(&env);
        Self::spend_allowance(&env, &from, &spender, amount);
        Self::spend_balance(&env, &from, amount);

        let total_shares = Self::read_total_shares(&env);
        let updated_total_shares = Self::checked_sub(&env, total_shares, amount);
        Self::write_total_shares(&env, updated_total_shares);

        TokenUtils::new(&env).events().burn(from, amount);
    }

    fn decimals(env: Env) -> u32 {
        Self::require_initialized(&env);
        Self::get_instance(&env, &DataKey::Decimals)
    }

    fn name(env: Env) -> String {
        Self::require_initialized(&env);
        Self::get_instance(&env, &DataKey::Name)
    }

    fn symbol(env: Env) -> String {
        Self::require_initialized(&env);
        Self::get_instance(&env, &DataKey::Symbol)
    }
}

impl HelixToken {
    fn assert_not_initialized(env: &Env) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, TokenError::AlreadyInitialized);
        }
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, TokenError::NotInitialized);
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
            None => panic_with_error!(env, TokenError::NotInitialized),
        }
    }

    fn admin(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Admin)
    }

    fn vault_contract(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::VaultContract)
    }

    fn bridge_handler(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::BridgeHandler)
    }

    fn read_total_shares(env: &Env) -> i128 {
        Self::get_instance(env, &DataKey::TotalShares)
    }

    fn write_total_shares(env: &Env, amount: i128) {
        env.storage().instance().set(&DataKey::TotalShares, &amount);
    }

    fn read_total_assets(env: &Env) -> i128 {
        Self::get_instance(env, &DataKey::TotalAssets)
    }

    fn balance_key(address: &Address) -> DataKey {
        DataKey::Balance(address.clone())
    }

    fn allowance_key(from: &Address, spender: &Address) -> DataKey {
        DataKey::Allowance(from.clone(), spender.clone())
    }

    fn read_balance(env: &Env, address: &Address) -> i128 {
        let key = Self::balance_key(address);
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    fn write_balance(env: &Env, address: &Address, amount: i128) {
        if amount < 0 {
            panic_with_error!(env, TokenError::InvalidAmount);
        }

        let key = Self::balance_key(address);
        if amount == 0 {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(&key, &amount);
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
        }
    }

    fn spend_balance(env: &Env, address: &Address, amount: i128) {
        let balance = Self::read_balance(env, address);
        if balance < amount {
            panic_with_error!(env, TokenError::InsufficientBalance);
        }

        let updated_balance = Self::checked_sub(env, balance, amount);
        Self::write_balance(env, address, updated_balance);
    }

    fn allowance_amount(env: &Env, from: &Address, spender: &Address) -> i128 {
        let key = Self::allowance_key(from, spender);
        match env.storage().temporary().get::<_, AllowanceData>(&key) {
            Some(allowance) if !Self::allowance_expired(env, &allowance) => allowance.amount,
            _ => 0,
        }
    }

    fn write_allowance(
        env: &Env,
        from: &Address,
        spender: &Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        let key = Self::allowance_key(from, spender);
        if amount == 0 {
            env.storage().temporary().remove(&key);
            return;
        }

        if expiration_ledger < env.ledger().sequence() {
            panic_with_error!(env, TokenError::InvalidAmount);
        }

        let allowance = AllowanceData {
            amount,
            expiration_ledger,
        };
        env.storage().temporary().set(&key, &allowance);
        env.storage()
            .temporary()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn spend_allowance(env: &Env, from: &Address, spender: &Address, amount: i128) {
        if amount == 0 {
            return;
        }

        let key = Self::allowance_key(from, spender);
        let allowance = match env.storage().temporary().get::<_, AllowanceData>(&key) {
            Some(allowance) => allowance,
            None => panic_with_error!(env, TokenError::InsufficientAllowance),
        };

        if Self::allowance_expired(env, &allowance) {
            panic_with_error!(env, TokenError::AllowanceExpired);
        }

        if allowance.amount < amount {
            panic_with_error!(env, TokenError::InsufficientAllowance);
        }

        let updated_amount = Self::checked_sub(env, allowance.amount, amount);
        Self::write_allowance(
            env,
            from,
            spender,
            updated_amount,
            allowance.expiration_ledger,
        );
    }

    fn allowance_expired(env: &Env, allowance: &AllowanceData) -> bool {
        allowance.expiration_ledger < env.ledger().sequence()
    }

    fn validate_amount(env: &Env, amount: i128) {
        if amount < 0 {
            panic_with_error!(env, TokenError::InvalidAmount);
        }
    }

    fn checked_add(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_add(right) {
            Some(value) => value,
            None => panic_with_error!(env, TokenError::InvalidAmount),
        }
    }

    fn checked_sub(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_sub(right) {
            Some(value) => value,
            None => panic_with_error!(env, TokenError::InvalidAmount),
        }
    }

    fn checked_mul(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_mul(right) {
            Some(value) => value,
            None => panic_with_error!(env, TokenError::InvalidAmount),
        }
    }
}

#[cfg(test)]
mod test;
