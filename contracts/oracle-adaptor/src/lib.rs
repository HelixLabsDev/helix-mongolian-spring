#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, vec, Address, BytesN,
    Env, Error, IntoVal, Symbol, TryFromVal, Val,
};

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;
const DEFAULT_TWAP_WINDOW: u64 = 1_800;
const DEFAULT_STALENESS_THRESHOLD: u64 = 600;
const DEFAULT_DEVIATION_THRESHOLD: u32 = 500;
const BPS_DENOMINATOR: i128 = 10_000;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    PrimaryOracle,
    SecondaryOracle,
    VaultCallback,
    TWAPWindow,
    StalenessThreshold,
    DeviationThreshold,
    CachedPrice(Address),
    TWAPState(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
    pub source: Symbol,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TWAPData {
    pub cumulative_price: i128,
    pub num_samples: u32,
    pub window_start: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleError {
    NotInitialized = 1,
    StalePrice = 2,
    DeviationExceeded = 3,
    FeedUnavailable = 4,
    Unauthorized = 5,
    AlreadyInitialized = 6,
}

#[contract]
pub struct HelixOracleAdaptor;

#[contractimpl]
impl HelixOracleAdaptor {
    pub fn initialize(
        env: Env,
        admin: Address,
        primary_oracle: Address,
        secondary_oracle: Address,
        vault_callback: Address,
    ) {
        Self::assert_not_initialized(&env);
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PrimaryOracle, &primary_oracle);
        env.storage()
            .instance()
            .set(&DataKey::SecondaryOracle, &secondary_oracle);
        env.storage()
            .instance()
            .set(&DataKey::VaultCallback, &vault_callback);
        env.storage()
            .instance()
            .set(&DataKey::TWAPWindow, &DEFAULT_TWAP_WINDOW);
        env.storage()
            .instance()
            .set(&DataKey::StalenessThreshold, &DEFAULT_STALENESS_THRESHOLD);
        env.storage()
            .instance()
            .set(&DataKey::DeviationThreshold, &DEFAULT_DEVIATION_THRESHOLD);
        Self::extend_instance(&env);
    }

    pub fn lastprice(env: Env, asset: Address) -> (i128, u64) {
        Self::require_initialized(&env);
        Self::current_twap(&env, &asset)
    }

    pub fn decimals(env: Env, asset: Address) -> u32 {
        Self::require_initialized(&env);

        let primary = Self::primary_oracle(&env);
        if let Some(decimals) = Self::try_oracle_decimals(&env, &primary, &asset) {
            return decimals;
        }

        let secondary = Self::secondary_oracle(&env);
        if let Some(decimals) = Self::try_oracle_decimals(&env, &secondary, &asset) {
            return decimals;
        }

        panic_with_error!(&env, OracleError::FeedUnavailable);
    }

    pub fn update_price(env: Env, asset: Address) {
        Self::require_initialized(&env);
        Self::extend_instance(&env);

        let primary_address = Self::primary_oracle(&env);
        let secondary_address = Self::secondary_oracle(&env);
        let target_decimals =
            Self::canonical_decimals(&env, &primary_address, &secondary_address, &asset);

        let primary = Self::fetch_observation(
            &env,
            &primary_address,
            &asset,
            target_decimals,
            Symbol::new(&env, "primary"),
        );
        let secondary = Self::fetch_observation(
            &env,
            &secondary_address,
            &asset,
            target_decimals,
            Symbol::new(&env, "secondary"),
        );

        let price = match (primary, secondary) {
            (Some(primary), Some(secondary)) => {
                if Self::deviation_exceeded(&env, primary.price, secondary.price) {
                    Self::set_safe_mode_internal(&env, true);
                    env.events().publish(
                        (Symbol::new(&env, "safe_mode"), asset.clone()),
                        (primary.price, secondary.price),
                    );
                    return;
                }
                Self::merge_observations(&env, primary, secondary)
            }
            (Some(primary), None) => primary,
            (None, Some(secondary)) => secondary,
            (None, None) => panic_with_error!(&env, OracleError::FeedUnavailable),
        };

        let now = env.ledger().timestamp();
        let mut state = Self::read_twap_state(&env, &asset).unwrap_or(TWAPData {
            cumulative_price: 0,
            num_samples: 0,
            window_start: now,
        });
        let window = Self::twap_window(&env);

        if state.num_samples == 0 || now.saturating_sub(state.window_start) >= window {
            state = TWAPData {
                cumulative_price: price.price,
                num_samples: 1,
                window_start: now,
            };
        } else {
            state.cumulative_price = Self::checked_add(&env, state.cumulative_price, price.price);
            state.num_samples = state.num_samples.saturating_add(1);
        }

        Self::write_cached_price(&env, &asset, &price);
        Self::write_twap_state(&env, &asset, &state);
        env.events().publish(
            (Symbol::new(&env, "price_update"), asset),
            (
                price.price,
                price.timestamp,
                price.source,
                state.num_samples,
            ),
        );
    }

    pub fn get_twap(env: Env, asset: Address, duration: u64) -> (i128, u64) {
        Self::require_initialized(&env);
        let _effective_duration = if duration == 0 {
            Self::twap_window(&env)
        } else {
            duration.min(Self::twap_window(&env))
        };
        Self::current_twap(&env, &asset)
    }

    pub fn set_safe_mode(env: Env, enabled: bool) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        Self::extend_instance(&env);
        Self::set_safe_mode_internal(&env, enabled);
    }

    pub fn configure(
        env: Env,
        primary: Address,
        secondary: Address,
        twap_window: u64,
        staleness: u64,
        deviation: u32,
    ) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);

        env.storage()
            .instance()
            .set(&DataKey::PrimaryOracle, &primary);
        env.storage()
            .instance()
            .set(&DataKey::SecondaryOracle, &secondary);
        env.storage()
            .instance()
            .set(&DataKey::TWAPWindow, &twap_window);
        env.storage()
            .instance()
            .set(&DataKey::StalenessThreshold, &staleness);
        env.storage()
            .instance()
            .set(&DataKey::DeviationThreshold, &deviation);
        Self::extend_instance(&env);

        env.events().publish(
            (Symbol::new(&env, "configure"),),
            (primary, secondary, twap_window, staleness, deviation),
        );
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::require_initialized(&env);
        Self::require_admin_auth(&env);
        Self::extend_instance(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

impl HelixOracleAdaptor {
    fn assert_not_initialized(env: &Env) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, OracleError::AlreadyInitialized);
        }
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, OracleError::NotInitialized);
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
            None => panic_with_error!(env, OracleError::NotInitialized),
        }
    }

    fn admin(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Admin)
    }

    fn primary_oracle(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::PrimaryOracle)
    }

    fn secondary_oracle(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::SecondaryOracle)
    }

    fn vault_callback(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::VaultCallback)
    }

    fn twap_window(env: &Env) -> u64 {
        Self::get_instance(env, &DataKey::TWAPWindow)
    }

    fn staleness_threshold(env: &Env) -> u64 {
        Self::get_instance(env, &DataKey::StalenessThreshold)
    }

    fn deviation_threshold(env: &Env) -> u32 {
        Self::get_instance(env, &DataKey::DeviationThreshold)
    }

    fn require_admin_auth(env: &Env) -> Address {
        let admin = Self::admin(env);
        admin.require_auth();
        admin
    }

    fn cached_price_key(asset: &Address) -> DataKey {
        DataKey::CachedPrice(asset.clone())
    }

    fn twap_state_key(asset: &Address) -> DataKey {
        DataKey::TWAPState(asset.clone())
    }

    fn read_cached_price(env: &Env, asset: &Address) -> Option<PriceData> {
        env.storage()
            .temporary()
            .get::<_, PriceData>(&Self::cached_price_key(asset))
    }

    fn read_twap_state(env: &Env, asset: &Address) -> Option<TWAPData> {
        env.storage()
            .temporary()
            .get::<_, TWAPData>(&Self::twap_state_key(asset))
    }

    fn write_cached_price(env: &Env, asset: &Address, price: &PriceData) {
        let key = Self::cached_price_key(asset);
        env.storage().temporary().set(&key, price);
        env.storage()
            .temporary()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn write_twap_state(env: &Env, asset: &Address, state: &TWAPData) {
        let key = Self::twap_state_key(asset);
        env.storage().temporary().set(&key, state);
        env.storage()
            .temporary()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn current_twap(env: &Env, asset: &Address) -> (i128, u64) {
        let cached = Self::validated_cached_price(env, asset);

        match Self::read_twap_state(env, asset) {
            Some(state) if state.num_samples > 0 => {
                let average = state.cumulative_price / i128::from(state.num_samples);
                (average, cached.timestamp)
            }
            _ => (cached.price, cached.timestamp),
        }
    }

    fn validated_cached_price(env: &Env, asset: &Address) -> PriceData {
        let cached = match Self::read_cached_price(env, asset) {
            Some(cached) => cached,
            None => panic_with_error!(env, OracleError::FeedUnavailable),
        };

        if env.ledger().timestamp().saturating_sub(cached.timestamp)
            > Self::staleness_threshold(env)
        {
            panic_with_error!(env, OracleError::StalePrice);
        }

        cached
    }

    fn canonical_decimals(
        env: &Env,
        primary: &Address,
        secondary: &Address,
        asset: &Address,
    ) -> u32 {
        if let Some(decimals) = Self::try_oracle_decimals(env, primary, asset) {
            return decimals;
        }

        if let Some(decimals) = Self::try_oracle_decimals(env, secondary, asset) {
            return decimals;
        }

        panic_with_error!(env, OracleError::FeedUnavailable);
    }

    fn fetch_observation(
        env: &Env,
        oracle: &Address,
        asset: &Address,
        target_decimals: u32,
        source: Symbol,
    ) -> Option<PriceData> {
        let oracle_decimals = Self::try_oracle_decimals(env, oracle, asset)?;
        let (price, timestamp) = Self::try_oracle_lastprice(env, oracle, asset)?;
        if price <= 0 {
            return None;
        }

        if env.ledger().timestamp().saturating_sub(timestamp) > Self::staleness_threshold(env) {
            return None;
        }

        Some(PriceData {
            price: Self::scale_price(env, price, oracle_decimals, target_decimals),
            timestamp,
            source,
        })
    }

    fn merge_observations(env: &Env, left: PriceData, right: PriceData) -> PriceData {
        PriceData {
            price: Self::checked_add(env, left.price, right.price) / 2,
            timestamp: left.timestamp.min(right.timestamp),
            source: Symbol::new(env, "blended"),
        }
    }

    fn try_oracle_lastprice(env: &Env, oracle: &Address, asset: &Address) -> Option<(i128, u64)> {
        match env.try_invoke_contract::<(i128, u64), Error>(
            oracle,
            &Symbol::new(env, "lastprice"),
            vec![env, asset.clone().into_val(env)],
        ) {
            Ok(Ok(value)) => Some(value),
            _ => None,
        }
    }

    fn try_oracle_decimals(env: &Env, oracle: &Address, asset: &Address) -> Option<u32> {
        match env.try_invoke_contract::<u32, Error>(
            oracle,
            &Symbol::new(env, "decimals"),
            vec![env, asset.clone().into_val(env)],
        ) {
            Ok(Ok(value)) => Some(value),
            _ => None,
        }
    }

    fn deviation_exceeded(env: &Env, left: i128, right: i128) -> bool {
        let difference = if left >= right {
            left - right
        } else {
            right - left
        };
        let baseline = if left <= right { left } else { right };
        if baseline <= 0 {
            return true;
        }

        let deviation_bps = Self::checked_mul(env, difference, BPS_DENOMINATOR) / baseline;
        deviation_bps > i128::from(Self::deviation_threshold(env))
    }

    fn scale_price(env: &Env, mut price: i128, from_decimals: u32, to_decimals: u32) -> i128 {
        if from_decimals == to_decimals {
            return price;
        }

        if from_decimals < to_decimals {
            let scale = to_decimals - from_decimals;
            let mut index = 0;
            while index < scale {
                price = Self::checked_mul(env, price, 10);
                index += 1;
            }
            return price;
        }

        let scale = from_decimals - to_decimals;
        let mut index = 0;
        while index < scale {
            price /= 10;
            index += 1;
        }
        price
    }

    fn set_safe_mode_internal(env: &Env, enabled: bool) {
        let vault = Self::vault_callback(env);
        let method = if enabled {
            Symbol::new(env, "pause_by")
        } else {
            Symbol::new(env, "unpause_by")
        };
        env.invoke_contract::<()>(
            &vault,
            &method,
            vec![env, env.current_contract_address().into_val(env)],
        );
    }

    fn checked_add(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_add(right) {
            Some(value) => value,
            None => panic_with_error!(env, OracleError::FeedUnavailable),
        }
    }

    fn checked_mul(env: &Env, left: i128, right: i128) -> i128 {
        match left.checked_mul(right) {
            Some(value) => value,
            None => panic_with_error!(env, OracleError::FeedUnavailable),
        }
    }
}

#[cfg(test)]
mod test;
