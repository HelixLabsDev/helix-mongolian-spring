#![cfg(test)]

extern crate std;

use sep_40_oracle::{Asset, PriceData, PriceFeedClient};
use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger as _},
    vec, Address, Env, Symbol, Vec,
};

use super::{DataKey, HelixBlendOracleAdaptor, HelixBlendOracleAdaptorClient};

const DECIMALS: u32 = 7;
const RESOLUTION: u32 = 300;

#[contracttype]
#[derive(Clone)]
enum MockHelixOracleDataKey {
    Price(Address),
    Decimals(Address),
}

#[contracttype]
#[derive(Clone)]
enum MockBlendPoolDataKey {
    Oracle,
    Asset,
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

    pub fn set_decimals(env: Env, asset: Address, decimals: u32) {
        env.storage()
            .instance()
            .set(&MockHelixOracleDataKey::Decimals(asset), &decimals);
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

    pub fn decimals(env: Env, asset: Address) -> u32 {
        env.storage()
            .instance()
            .get::<_, u32>(&MockHelixOracleDataKey::Decimals(asset))
            .unwrap_or(DECIMALS)
    }
}

#[contract]
struct MockBlendPool;

#[contractimpl]
impl MockBlendPool {
    pub fn initialize(env: Env, oracle: Address, asset: Address) {
        env.storage()
            .instance()
            .set(&MockBlendPoolDataKey::Oracle, &oracle);
        env.storage()
            .instance()
            .set(&MockBlendPoolDataKey::Asset, &asset);
    }

    pub fn load_price_decimals(env: Env) -> u32 {
        let oracle = Self::oracle(&env);
        let oracle_client = PriceFeedClient::new(&env, &oracle);
        oracle_client.decimals()
    }

    pub fn load_price(env: Env) -> Option<PriceData> {
        let oracle = Self::oracle(&env);
        let asset = Self::asset(&env);
        let oracle_client = PriceFeedClient::new(&env, &oracle);
        oracle_client.lastprice(&Asset::Stellar(asset))
    }
}

impl MockBlendPool {
    fn oracle(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&MockBlendPoolDataKey::Oracle)
            .expect("oracle must be initialized")
    }

    fn asset(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&MockBlendPoolDataKey::Asset)
            .expect("asset must be initialized")
    }
}

struct BlendOracleAdaptorFixture<'a> {
    env: Env,
    client: HelixBlendOracleAdaptorClient<'a>,
    price_feed: PriceFeedClient<'a>,
    helix_oracle: MockHelixOracleClient<'a>,
    admin: Address,
    hsteth: Address,
    unsupported: Address,
}

impl<'a> BlendOracleAdaptorFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_700_000_000);
        env.ledger().set_sequence_number(100);

        let admin = Address::generate(&env);
        let hsteth = Address::generate(&env);
        let unsupported = Address::generate(&env);
        let helix_oracle_id = env.register(MockHelixOracle, ());
        let adaptor_id = env.register(HelixBlendOracleAdaptor, ());

        let client = HelixBlendOracleAdaptorClient::new(&env, &adaptor_id);
        let price_feed = PriceFeedClient::new(&env, &adaptor_id);
        let helix_oracle = MockHelixOracleClient::new(&env, &helix_oracle_id);
        let base = Asset::Other(Symbol::new(&env, "USD"));
        let assets = vec![&env, Asset::Stellar(hsteth.clone())];

        env.mock_all_auths();
        client.initialize(
            &admin,
            &helix_oracle_id,
            &base,
            &assets,
            &DECIMALS,
            &RESOLUTION,
        );
        helix_oracle.set_decimals(&hsteth, &DECIMALS);
        env.set_auths(&[]);

        Self {
            env,
            client,
            price_feed,
            helix_oracle,
            admin,
            hsteth,
            unsupported,
        }
    }

    fn set_price(&self, price: i128, timestamp: u64) {
        self.helix_oracle
            .set_price(&self.hsteth, &price, &timestamp);
    }
}

fn instance_value<T>(env: &Env, contract: &Address, key: &DataKey) -> T
where
    T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
{
    env.as_contract(contract, || {
        env.storage()
            .instance()
            .get::<_, T>(key)
            .expect("instance value must exist")
    })
}

fn assert_stellar_asset(asset: Asset, expected: &Address) {
    match asset {
        Asset::Stellar(address) => assert_eq!(address, *expected),
        Asset::Other(_) => panic!("expected Stellar asset"),
    }
}

fn assert_price_data(actual: Option<PriceData>, expected_price: i128, expected_timestamp: u64) {
    let Some(price_data) = actual else {
        panic!("expected price data");
    };
    assert_eq!(price_data.price, expected_price);
    assert_eq!(price_data.timestamp, expected_timestamp);
}

#[test]
fn test_initialize_stores_sep40_config() {
    let fixture = BlendOracleAdaptorFixture::new();

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Admin),
        fixture.admin
    );
    assert_eq!(fixture.price_feed.decimals(), DECIMALS);
    assert_eq!(fixture.price_feed.resolution(), RESOLUTION);
    let assets = fixture.price_feed.assets();
    assert_eq!(assets.len(), 1);
    assert_stellar_asset(assets.get_unchecked(0), &fixture.hsteth);
}

#[test]
fn test_sep40_lastprice_returns_helix_price_data() {
    let fixture = BlendOracleAdaptorFixture::new();
    fixture.set_price(2_345_678_900, 1_700_000_100);

    assert_price_data(
        fixture
            .price_feed
            .lastprice(&Asset::Stellar(fixture.hsteth.clone())),
        2_345_678_900,
        1_700_000_100,
    );
}

#[test]
fn test_sep40_price_returns_exact_timestamp_only() {
    let fixture = BlendOracleAdaptorFixture::new();
    fixture.set_price(2_345_678_900, 1_700_000_100);

    assert_price_data(
        fixture
            .price_feed
            .price(&Asset::Stellar(fixture.hsteth.clone()), &1_700_000_100),
        2_345_678_900,
        1_700_000_100,
    );
    assert!(fixture
        .price_feed
        .price(&Asset::Stellar(fixture.hsteth.clone()), &1_700_000_099)
        .is_none());
}

#[test]
fn test_sep40_prices_returns_latest_record() {
    let fixture = BlendOracleAdaptorFixture::new();
    fixture.set_price(2_345_678_900, 1_700_000_100);

    let prices = fixture
        .price_feed
        .prices(&Asset::Stellar(fixture.hsteth.clone()), &1)
        .expect("expected price records");
    assert_eq!(prices.len(), 1);
    let price_data = prices.get_unchecked(0);
    assert_eq!(price_data.price, 2_345_678_900);
    assert_eq!(price_data.timestamp, 1_700_000_100);
    assert!(fixture
        .price_feed
        .prices(&Asset::Stellar(fixture.hsteth.clone()), &0)
        .is_none());
}

#[test]
fn test_unsupported_assets_return_none() {
    let fixture = BlendOracleAdaptorFixture::new();
    fixture.set_price(2_345_678_900, 1_700_000_100);

    assert!(fixture
        .price_feed
        .lastprice(&Asset::Stellar(fixture.unsupported.clone()))
        .is_none());
    assert!(fixture
        .price_feed
        .lastprice(&Asset::Other(Symbol::new(&fixture.env, "ETH")))
        .is_none());
}

#[test]
fn test_zero_price_returns_none() {
    let fixture = BlendOracleAdaptorFixture::new();
    fixture.set_price(0, 1_700_000_100);

    assert!(fixture
        .price_feed
        .lastprice(&Asset::Stellar(fixture.hsteth.clone()))
        .is_none());
}

#[test]
fn test_blend_pool_constructor_read_smoke() {
    let fixture = BlendOracleAdaptorFixture::new();
    fixture.set_price(2_345_678_900, 1_700_000_100);
    let pool_id = fixture.env.register(MockBlendPool, ());
    let pool = MockBlendPoolClient::new(&fixture.env, &pool_id);

    pool.initialize(&fixture.client.address, &fixture.hsteth);

    assert_eq!(pool.load_price_decimals(), DECIMALS);
    assert_price_data(pool.load_price(), 2_345_678_900, 1_700_000_100);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_initialize_twice_fails() {
    let fixture = BlendOracleAdaptorFixture::new();
    let base = Asset::Other(Symbol::new(&fixture.env, "USD"));
    let assets = vec![&fixture.env, Asset::Stellar(fixture.hsteth.clone())];
    fixture.env.mock_all_auths();

    fixture.client.initialize(
        &fixture.admin,
        &fixture.helix_oracle.address,
        &base,
        &assets,
        &DECIMALS,
        &RESOLUTION,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_empty_assets_rejected() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let helix_oracle = Address::generate(&env);
    let adaptor_id = env.register(HelixBlendOracleAdaptor, ());
    let client = HelixBlendOracleAdaptorClient::new(&env, &adaptor_id);
    let base = Asset::Other(Symbol::new(&env, "USD"));
    env.mock_all_auths();

    client.initialize(
        &admin,
        &helix_oracle,
        &base,
        &Vec::new(&env),
        &DECIMALS,
        &RESOLUTION,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_zero_resolution_rejected() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let hsteth = Address::generate(&env);
    let helix_oracle = Address::generate(&env);
    let adaptor_id = env.register(HelixBlendOracleAdaptor, ());
    let client = HelixBlendOracleAdaptorClient::new(&env, &adaptor_id);
    let base = Asset::Other(Symbol::new(&env, "USD"));
    let assets = vec![&env, Asset::Stellar(hsteth)];
    env.mock_all_auths();

    client.initialize(&admin, &helix_oracle, &base, &assets, &DECIMALS, &0);
}
