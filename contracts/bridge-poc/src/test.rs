#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env, String, Val};

use super::{BridgePoc, BridgePocClient, DataKey};

struct BridgePocTestFixture<'a> {
    env: Env,
    client: BridgePocClient<'a>,
    gateway: Address,
    gas_service: Address,
}

impl<'a> BridgePocTestFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        let gateway = Address::generate(&env);
        let gas_service = Address::generate(&env);
        let contract_id = env.register(BridgePoc, (&gateway, &gas_service));
        let client = BridgePocClient::new(&env, &contract_id);

        Self {
            env,
            client,
            gateway,
            gas_service,
        }
    }
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

#[test]
fn test_constructor_stores_gateway_and_gas_service() {
    let fixture = BridgePocTestFixture::new();

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Gateway),
        fixture.gateway
    );
    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::GasService),
        fixture.gas_service
    );
}

#[test]
fn test_received_message_is_none_before_execute() {
    let fixture = BridgePocTestFixture::new();

    assert_eq!(fixture.client.received_message(), None::<String>);
}
