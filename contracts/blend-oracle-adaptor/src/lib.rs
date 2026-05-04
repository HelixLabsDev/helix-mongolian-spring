#![no_std]

use sep_40_oracle::{Asset, PriceData, PriceFeedTrait};
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, Address,
    Env, TryFromVal, Val, Vec,
};

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    HelixOracle,
    BaseAsset,
    Assets,
    Decimals,
    Resolution,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BlendOracleError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    InvalidConfig = 3,
}

#[contractclient(name = "HelixOracleClient")]
pub trait HelixOracleInterface {
    fn lastprice(env: Env, asset: Address) -> (i128, u64);
    fn decimals(env: Env, asset: Address) -> u32;
}

#[contract]
pub struct HelixBlendOracleAdaptor;

#[contractimpl]
impl HelixBlendOracleAdaptor {
    pub fn initialize(
        env: Env,
        admin: Address,
        helix_oracle: Address,
        base: Asset,
        assets: Vec<Asset>,
        decimals: u32,
        resolution: u32,
    ) {
        Self::assert_not_initialized(&env);
        admin.require_auth();
        if assets.is_empty() || resolution == 0 {
            panic_with_error!(&env, BlendOracleError::InvalidConfig);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::HelixOracle, &helix_oracle);
        env.storage().instance().set(&DataKey::BaseAsset, &base);
        env.storage().instance().set(&DataKey::Assets, &assets);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage()
            .instance()
            .set(&DataKey::Resolution, &resolution);
        Self::extend_instance(&env);
    }

    pub fn set_helix_oracle(env: Env, helix_oracle: Address) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        env.storage()
            .instance()
            .set(&DataKey::HelixOracle, &helix_oracle);
        Self::extend_instance(&env);
    }

    pub fn set_assets(env: Env, assets: Vec<Asset>) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        if assets.is_empty() {
            panic_with_error!(&env, BlendOracleError::InvalidConfig);
        }

        env.storage().instance().set(&DataKey::Assets, &assets);
        Self::extend_instance(&env);
    }
}

#[contractimpl]
impl PriceFeedTrait for HelixBlendOracleAdaptor {
    fn base(env: Env) -> Asset {
        Self::require_initialized(&env);
        Self::get_instance(&env, &DataKey::BaseAsset)
    }

    fn assets(env: Env) -> Vec<Asset> {
        Self::require_initialized(&env);
        Self::supported_assets(&env)
    }

    fn decimals(env: Env) -> u32 {
        Self::require_initialized(&env);
        Self::get_instance(&env, &DataKey::Decimals)
    }

    fn resolution(env: Env) -> u32 {
        Self::require_initialized(&env);
        Self::get_instance(&env, &DataKey::Resolution)
    }

    fn price(env: Env, asset: Asset, timestamp: u64) -> Option<PriceData> {
        let latest = Self::lastprice(env, asset)?;
        if latest.timestamp == timestamp {
            Some(latest)
        } else {
            None
        }
    }

    fn prices(env: Env, asset: Asset, records: u32) -> Option<Vec<PriceData>> {
        if records == 0 {
            return None;
        }

        let latest = Self::lastprice(env.clone(), asset)?;
        let mut prices = Vec::new(&env);
        prices.push_back(latest);
        Some(prices)
    }

    fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        Self::require_initialized(&env);
        let Asset::Stellar(asset_address) = asset else {
            return None;
        };
        if !Self::contains_asset(&env, &asset_address) {
            return None;
        }

        let helix_oracle = Self::helix_oracle(&env);
        let helix_client = HelixOracleClient::new(&env, &helix_oracle);
        let (price, timestamp) = helix_client.lastprice(&asset_address);
        if price <= 0 {
            return None;
        }

        Some(PriceData { price, timestamp })
    }
}

impl HelixBlendOracleAdaptor {
    fn assert_not_initialized(env: &Env) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, BlendOracleError::AlreadyInitialized);
        }
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, BlendOracleError::NotInitialized);
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
            None => panic_with_error!(env, BlendOracleError::NotInitialized),
        }
    }

    fn admin(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Admin)
    }

    fn helix_oracle(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::HelixOracle)
    }

    fn supported_assets(env: &Env) -> Vec<Asset> {
        Self::get_instance(env, &DataKey::Assets)
    }

    fn contains_asset(env: &Env, target: &Address) -> bool {
        for asset in Self::supported_assets(env).iter() {
            if let Asset::Stellar(address) = asset {
                if address == *target {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod test;
