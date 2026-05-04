#![cfg(test)]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error,
    testutils::{
        storage::{Instance as _, Temporary as _},
        Address as _, Ledger as _,
    },
    Address, Env, TryFromVal, Val,
};

use super::{
    DataKey, HelixOracleAdaptor, HelixOracleAdaptorClient, PriceData, TWAPData,
    DEFAULT_DEVIATION_THRESHOLD, DEFAULT_STALENESS_THRESHOLD, DEFAULT_TWAP_WINDOW, TTL_BUMP,
    TTL_THRESHOLD,
};

const ORACLE_DECIMALS: u32 = 7;

#[contracttype]
#[derive(Clone)]
enum MockOracleDataKey {
    Price(Address),
    Timestamp(Address),
    Available,
    Decimals,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum MockOracleError {
    FeedUnavailable = 1,
}

#[contract]
struct MockOracle;

#[contractimpl]
impl MockOracle {
    pub fn set_price(env: Env, asset: Address, price: i128, timestamp: u64) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        env.storage()
            .instance()
            .set(&MockOracleDataKey::Price(asset.clone()), &price);
        env.storage()
            .instance()
            .set(&MockOracleDataKey::Timestamp(asset), &timestamp);
    }

    pub fn set_available(env: Env, available: bool) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        env.storage()
            .instance()
            .set(&MockOracleDataKey::Available, &available);
    }

    pub fn set_decimals(env: Env, decimals: u32) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        env.storage()
            .instance()
            .set(&MockOracleDataKey::Decimals, &decimals);
    }

    pub fn lastprice(env: Env, asset: Address) -> (i128, u64) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        if !env
            .storage()
            .instance()
            .get::<_, bool>(&MockOracleDataKey::Available)
            .unwrap_or(true)
        {
            panic_with_error!(&env, MockOracleError::FeedUnavailable);
        }

        let price = env
            .storage()
            .instance()
            .get::<_, i128>(&MockOracleDataKey::Price(asset.clone()))
            .unwrap_or(0);
        let timestamp = env
            .storage()
            .instance()
            .get::<_, u64>(&MockOracleDataKey::Timestamp(asset))
            .unwrap_or(env.ledger().timestamp());
        (price, timestamp)
    }

    pub fn decimals(env: Env, _asset: Address) -> u32 {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        if !env
            .storage()
            .instance()
            .get::<_, bool>(&MockOracleDataKey::Available)
            .unwrap_or(true)
        {
            panic_with_error!(&env, MockOracleError::FeedUnavailable);
        }

        env.storage()
            .instance()
            .get::<_, u32>(&MockOracleDataKey::Decimals)
            .unwrap_or(ORACLE_DECIMALS)
    }
}

#[contracttype]
#[derive(Clone)]
enum MockVaultDataKey {
    Admin,
    Paused,
}

#[contract]
struct MockVault;

#[contractimpl]
impl MockVault {
    pub fn initialize(env: Env, admin: Address) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        env.storage()
            .instance()
            .set(&MockVaultDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&MockVaultDataKey::Paused, &false);
    }

    pub fn pause(env: Env) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&MockVaultDataKey::Admin)
            .expect("vault admin must exist");
        admin.require_auth();
        env.storage()
            .instance()
            .set(&MockVaultDataKey::Paused, &true);
    }

    pub fn pause_by(env: Env, caller: Address) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&MockVaultDataKey::Admin)
            .expect("vault admin must exist");
        if caller != admin {
            panic_with_error!(&env, MockOracleError::FeedUnavailable);
        }
        caller.require_auth();
        env.storage()
            .instance()
            .set(&MockVaultDataKey::Paused, &true);
    }

    pub fn unpause(env: Env) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&MockVaultDataKey::Admin)
            .expect("vault admin must exist");
        admin.require_auth();
        env.storage()
            .instance()
            .set(&MockVaultDataKey::Paused, &false);
    }

    pub fn unpause_by(env: Env, caller: Address) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&MockVaultDataKey::Admin)
            .expect("vault admin must exist");
        if caller != admin {
            panic_with_error!(&env, MockOracleError::FeedUnavailable);
        }
        caller.require_auth();
        env.storage()
            .instance()
            .set(&MockVaultDataKey::Paused, &false);
    }

    pub fn paused(env: Env) -> bool {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
        env.storage()
            .instance()
            .get::<_, bool>(&MockVaultDataKey::Paused)
            .unwrap_or(false)
    }
}

struct OracleAdaptorTestFixture<'a> {
    env: Env,
    client: HelixOracleAdaptorClient<'a>,
    primary: MockOracleClient<'a>,
    secondary: MockOracleClient<'a>,
    vault: MockVaultClient<'a>,
    admin: Address,
    asset: Address,
}

impl<'a> OracleAdaptorTestFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.ledger().set_sequence_number(100);
        env.ledger().set_timestamp(1_700_000_000);
        env.ledger().set_max_entry_ttl(TTL_BUMP + 1_000);
        env.ledger().set_min_temp_entry_ttl(1);
        env.ledger().set_min_persistent_entry_ttl(1);

        let admin = Address::generate(&env);
        let asset = Address::generate(&env);

        let adaptor_id = env.register(HelixOracleAdaptor, ());
        let primary_id = env.register(MockOracle, ());
        let secondary_id = env.register(MockOracle, ());
        let vault_id = env.register(MockVault, ());

        let client = HelixOracleAdaptorClient::new(&env, &adaptor_id);
        let primary = MockOracleClient::new(&env, &primary_id);
        let secondary = MockOracleClient::new(&env, &secondary_id);
        let vault = MockVaultClient::new(&env, &vault_id);

        env.mock_all_auths_allowing_non_root_auth();
        vault.initialize(&adaptor_id);
        primary.set_decimals(&ORACLE_DECIMALS);
        secondary.set_decimals(&ORACLE_DECIMALS);
        secondary.set_available(&true);
        client.initialize(&admin, &primary_id, &secondary_id, &vault_id);
        env.set_auths(&[]);

        Self {
            env,
            client,
            primary,
            secondary,
            vault,
            admin,
            asset,
        }
    }

    fn with_all_auths<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.env.mock_all_auths_allowing_non_root_auth();
        let result = f();
        self.env.set_auths(&[]);
        result
    }

    fn set_primary_price(&self, price: i128) {
        self.with_all_auths(|| {
            self.primary
                .set_price(&self.asset, &price, &self.env.ledger().timestamp())
        });
    }

    fn set_secondary_price(&self, price: i128) {
        self.with_all_auths(|| {
            self.secondary
                .set_price(&self.asset, &price, &self.env.ledger().timestamp())
        });
    }

    fn set_secondary_available(&self, available: bool) {
        self.with_all_auths(|| self.secondary.set_available(&available));
    }

    fn configure(&self, twap_window: u64, staleness: u64, deviation: u32) {
        self.with_all_auths(|| {
            self.client.configure(
                &self.primary.address,
                &self.secondary.address,
                &twap_window,
                &staleness,
                &deviation,
            )
        });
    }

    fn set_safe_mode(&self, enabled: bool) {
        self.with_all_auths(|| self.client.set_safe_mode(&enabled));
    }

    fn advance(&self, ledgers: u32, seconds: u64) {
        self.env
            .ledger()
            .set_sequence_number(self.env.ledger().sequence() + ledgers);
        self.env
            .ledger()
            .set_timestamp(self.env.ledger().timestamp() + seconds);
    }
}

fn instance_value<T>(env: &Env, contract: &Address, key: &DataKey) -> T
where
    T: TryFromVal<Env, Val>,
{
    env.as_contract(contract, || {
        env.storage()
            .instance()
            .get::<_, T>(key)
            .expect("instance value must exist")
    })
}

fn temporary_value<T>(env: &Env, contract: &Address, key: &DataKey) -> Option<T>
where
    T: TryFromVal<Env, Val>,
{
    env.as_contract(contract, || env.storage().temporary().get::<_, T>(key))
}

fn temporary_ttl(env: &Env, contract: &Address, key: &DataKey) -> u32 {
    env.as_contract(contract, || env.storage().temporary().get_ttl(key))
}

fn instance_ttl(env: &Env, contract: &Address) -> u32 {
    env.as_contract(contract, || env.storage().instance().get_ttl())
}

#[test]
fn test_initialize_and_configure() {
    let fixture = OracleAdaptorTestFixture::new();

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Admin),
        fixture.admin
    );
    assert_eq!(
        instance_value::<Address>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::PrimaryOracle
        ),
        fixture.primary.address
    );
    assert_eq!(
        instance_value::<Address>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::SecondaryOracle
        ),
        fixture.secondary.address
    );
    assert_eq!(
        instance_value::<Address>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::VaultCallback
        ),
        fixture.vault.address
    );
    assert_eq!(
        instance_value::<u64>(&fixture.env, &fixture.client.address, &DataKey::TWAPWindow),
        DEFAULT_TWAP_WINDOW
    );
    assert_eq!(
        instance_value::<u64>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::StalenessThreshold
        ),
        DEFAULT_STALENESS_THRESHOLD
    );
    assert_eq!(
        instance_value::<u32>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::DeviationThreshold
        ),
        DEFAULT_DEVIATION_THRESHOLD
    );

    fixture.configure(900, 300, 250);

    assert_eq!(
        instance_value::<u64>(&fixture.env, &fixture.client.address, &DataKey::TWAPWindow),
        900
    );
    assert_eq!(
        instance_value::<u64>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::StalenessThreshold
        ),
        300
    );
    assert_eq!(
        instance_value::<u32>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::DeviationThreshold
        ),
        250
    );
}

#[test]
fn test_update_price_single_source_lastprice_returns_twap() {
    let fixture = OracleAdaptorTestFixture::new();
    fixture.set_primary_price(100_000_000);
    fixture.set_secondary_available(false);

    fixture.client.update_price(&fixture.asset);

    assert_eq!(
        fixture.client.lastprice(&fixture.asset),
        (100_000_000, fixture.env.ledger().timestamp())
    );
}

#[test]
fn test_update_price_dual_source_without_deviation() {
    let fixture = OracleAdaptorTestFixture::new();
    fixture.set_primary_price(100_000_000);
    fixture.set_secondary_price(102_000_000);

    fixture.client.update_price(&fixture.asset);

    assert_eq!(
        fixture.client.lastprice(&fixture.asset),
        (101_000_000, fixture.env.ledger().timestamp())
    );
    assert!(!fixture.vault.paused());
}

#[test]
fn test_update_price_dual_source_with_deviation_triggers_safe_mode() {
    let fixture = OracleAdaptorTestFixture::new();
    fixture.set_primary_price(100_000_000);
    fixture.set_secondary_price(106_000_000);

    fixture.client.update_price(&fixture.asset);

    assert!(fixture.vault.paused());
    assert_eq!(
        temporary_value::<PriceData>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::CachedPrice(fixture.asset.clone())
        ),
        None
    );
}

#[test]
fn test_staleness_rejection() {
    let fixture = OracleAdaptorTestFixture::new();
    fixture.set_primary_price(100_000_000);
    fixture.set_secondary_available(false);
    fixture.client.update_price(&fixture.asset);

    fixture.advance(1, DEFAULT_STALENESS_THRESHOLD + 1);

    let result = catch_unwind(AssertUnwindSafe(|| {
        fixture.client.lastprice(&fixture.asset)
    }));
    assert!(result.is_err());
}

#[test]
fn test_twap_window_reset() {
    let fixture = OracleAdaptorTestFixture::new();
    fixture.set_secondary_available(false);

    fixture.set_primary_price(100_000_000);
    fixture.client.update_price(&fixture.asset);

    fixture.advance(1, 60);
    fixture.set_primary_price(110_000_000);
    fixture.client.update_price(&fixture.asset);

    assert_eq!(
        fixture.client.lastprice(&fixture.asset),
        (105_000_000, fixture.env.ledger().timestamp())
    );

    fixture.advance(1, DEFAULT_TWAP_WINDOW + 1);
    fixture.set_primary_price(140_000_000);
    fixture.client.update_price(&fixture.asset);

    assert_eq!(
        fixture.client.lastprice(&fixture.asset),
        (140_000_000, fixture.env.ledger().timestamp())
    );

    assert_eq!(
        temporary_value::<TWAPData>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::TWAPState(fixture.asset.clone())
        ),
        Some(TWAPData {
            cumulative_price: 140_000_000,
            num_samples: 1,
            window_start: fixture.env.ledger().timestamp(),
        })
    );
}

#[test]
fn test_sep_40_compliance() {
    let fixture = OracleAdaptorTestFixture::new();
    fixture.set_primary_price(123_456_789);
    fixture.set_secondary_available(false);
    fixture.client.update_price(&fixture.asset);

    let (price, timestamp) = fixture.client.lastprice(&fixture.asset);
    assert_eq!(price, 123_456_789);
    assert_eq!(timestamp, fixture.env.ledger().timestamp());
    assert_eq!(fixture.client.decimals(&fixture.asset), ORACLE_DECIMALS);
}

#[test]
fn test_unauthorized_configure_rejected() {
    let fixture = OracleAdaptorTestFixture::new();

    let result = catch_unwind(AssertUnwindSafe(|| {
        fixture.client.configure(
            &fixture.primary.address,
            &fixture.secondary.address,
            &DEFAULT_TWAP_WINDOW,
            &DEFAULT_STALENESS_THRESHOLD,
            &DEFAULT_DEVIATION_THRESHOLD,
        )
    }));

    assert!(result.is_err());
}

#[test]
fn test_ttl_extension_on_state_changing_calls() {
    let fixture = OracleAdaptorTestFixture::new();
    assert_eq!(
        instance_ttl(&fixture.env, &fixture.client.address),
        TTL_BUMP
    );

    fixture.advance(TTL_BUMP - TTL_THRESHOLD + 1, 0);
    assert!(instance_ttl(&fixture.env, &fixture.client.address) < TTL_THRESHOLD);

    fixture.configure(
        DEFAULT_TWAP_WINDOW,
        DEFAULT_STALENESS_THRESHOLD,
        DEFAULT_DEVIATION_THRESHOLD,
    );
    assert_eq!(
        instance_ttl(&fixture.env, &fixture.client.address),
        TTL_BUMP
    );

    fixture.set_primary_price(100_000_000);
    fixture.set_secondary_available(false);
    fixture.client.update_price(&fixture.asset);

    let cached_key = DataKey::CachedPrice(fixture.asset.clone());
    let twap_key = DataKey::TWAPState(fixture.asset.clone());
    assert_eq!(
        temporary_ttl(&fixture.env, &fixture.client.address, &cached_key),
        TTL_BUMP
    );
    assert_eq!(
        temporary_ttl(&fixture.env, &fixture.client.address, &twap_key),
        TTL_BUMP
    );

    assert!(!fixture.vault.paused());

    fixture.advance(TTL_BUMP - TTL_THRESHOLD + 1, 0);
    fixture.set_safe_mode(true);
    assert_eq!(
        instance_ttl(&fixture.env, &fixture.client.address),
        TTL_BUMP
    );
    assert!(fixture.vault.paused());

    fixture.advance(TTL_BUMP - TTL_THRESHOLD + 1, 0);
    fixture.set_safe_mode(false);
    assert_eq!(
        instance_ttl(&fixture.env, &fixture.client.address),
        TTL_BUMP
    );
    assert!(!fixture.vault.paused());
}
