#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
    TryFromVal, Val,
};

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;
const DEFAULT_DECIMALS: u32 = 7;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Price(Address),
    Decimals(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceEntry {
    pub price: i128,
    pub timestamp: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MockOracleError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
}

#[contract]
pub struct HelixMockOracle;

#[contractimpl]
impl HelixMockOracle {
    pub fn __constructor(env: Env, admin: Address) {
        Self::assert_not_initialized(&env);

        env.storage().instance().set(&DataKey::Admin, &admin);
        Self::extend_instance(&env);
    }

    pub fn set_price(env: Env, asset: Address, price: i128, timestamp: u64) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();

        env.storage()
            .instance()
            .set(&DataKey::Price(asset), &PriceEntry { price, timestamp });
        Self::extend_instance(&env);
    }

    pub fn set_decimals(env: Env, asset: Address, decimals: u32) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();

        env.storage()
            .instance()
            .set(&DataKey::Decimals(asset), &decimals);
        Self::extend_instance(&env);
    }

    pub fn lastprice(env: Env, asset: Address) -> (i128, u64) {
        Self::require_initialized(&env);

        let entry = env
            .storage()
            .instance()
            .get::<_, PriceEntry>(&DataKey::Price(asset))
            .unwrap_or(PriceEntry {
                price: 0,
                timestamp: env.ledger().timestamp(),
            });
        (entry.price, entry.timestamp)
    }

    pub fn decimals(env: Env, asset: Address) -> u32 {
        Self::require_initialized(&env);

        env.storage()
            .instance()
            .get::<_, u32>(&DataKey::Decimals(asset))
            .unwrap_or(DEFAULT_DECIMALS)
    }
}

impl HelixMockOracle {
    fn assert_not_initialized(env: &Env) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, MockOracleError::AlreadyInitialized);
        }
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, MockOracleError::NotInitialized);
        }
    }

    fn admin(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Admin)
    }

    fn get_instance<T>(env: &Env, key: &DataKey) -> T
    where
        T: TryFromVal<Env, Val>,
    {
        env.storage()
            .instance()
            .get::<_, T>(key)
            .expect("instance value must exist")
    }

    fn extend_instance(env: &Env) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
    }
}

#[cfg(test)]
mod test;
