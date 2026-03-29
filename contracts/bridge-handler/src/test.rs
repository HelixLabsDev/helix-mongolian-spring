#![cfg(test)]

extern crate std;

use std::{
    format,
    panic::{catch_unwind, AssertUnwindSafe},
    string::{String as StdString, ToString},
};

use soroban_sdk::{
    contract, contractimpl, contracttype,
    testutils::{Address as _, Ledger, LedgerInfo},
    xdr::{Hash, ScAddress},
    Address, Bytes, BytesN, Env, String, TryFromVal, Val,
};

use super::{
    BridgeHandler, BridgeHandlerClient, DataKey, DepositMessage, GasServiceError, GasToken,
    MESSAGE_TYPE_DEPOSIT, MESSAGE_TYPE_WITHDRAW, MESSAGE_TYPE_YIELD_UPDATE, TTL_BUMP,
};
use alloy_primitives::{FixedBytes, U256};
use alloy_sol_types::SolValue;

#[contracttype]
#[derive(Clone)]
enum MockGatewayDataKey {
    Approved,
    LastDestinationChain,
    LastDestinationAddress,
    LastPayload,
}

#[contract]
struct MockGateway;

#[contractimpl]
impl MockGateway {
    pub fn set_approved(env: Env, approved: bool) {
        env.storage()
            .instance()
            .set(&MockGatewayDataKey::Approved, &approved);
    }

    pub fn call_contract(
        env: Env,
        _caller: Address,
        destination_chain: String,
        destination_address: String,
        payload: Bytes,
    ) {
        env.storage().instance().set(
            &MockGatewayDataKey::LastDestinationChain,
            &destination_chain,
        );
        env.storage().instance().set(
            &MockGatewayDataKey::LastDestinationAddress,
            &destination_address,
        );
        env.storage()
            .instance()
            .set(&MockGatewayDataKey::LastPayload, &payload);
    }

    pub fn validate_message(
        env: Env,
        _caller: Address,
        _source_chain: String,
        _message_id: String,
        _source_address: String,
        _payload_hash: BytesN<32>,
    ) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&MockGatewayDataKey::Approved)
            .unwrap_or(true)
    }

    pub fn gateway_payload(env: Env) -> Option<Bytes> {
        env.storage()
            .instance()
            .get::<_, Bytes>(&MockGatewayDataKey::LastPayload)
    }

    pub fn last_destination_chain(env: Env) -> Option<String> {
        env.storage()
            .instance()
            .get::<_, String>(&MockGatewayDataKey::LastDestinationChain)
    }

    pub fn last_destination_address(env: Env) -> Option<String> {
        env.storage()
            .instance()
            .get::<_, String>(&MockGatewayDataKey::LastDestinationAddress)
    }
}

#[contracttype]
#[derive(Clone)]
enum MockGasServiceDataKey {
    DestinationChain,
    DestinationAddress,
    Payload,
    Spender,
    TokenAmount,
}

#[contract]
struct MockGasService;

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl MockGasService {
    #[allow(clippy::too_many_arguments)]
    pub fn pay_gas(
        env: Env,
        _sender: Address,
        destination_chain: String,
        destination_address: String,
        payload: Bytes,
        spender: Address,
        token: GasToken,
        _metadata: Bytes,
    ) -> Result<(), GasServiceError> {
        env.storage()
            .instance()
            .set(&MockGasServiceDataKey::DestinationChain, &destination_chain);
        env.storage().instance().set(
            &MockGasServiceDataKey::DestinationAddress,
            &destination_address,
        );
        env.storage()
            .instance()
            .set(&MockGasServiceDataKey::Payload, &payload);
        env.storage()
            .instance()
            .set(&MockGasServiceDataKey::Spender, &spender);
        env.storage()
            .instance()
            .set(&MockGasServiceDataKey::TokenAmount, &token.amount);
        Ok(())
    }

    pub fn gas_payload(env: Env) -> Option<Bytes> {
        env.storage()
            .instance()
            .get::<_, Bytes>(&MockGasServiceDataKey::Payload)
    }
}

#[contracttype]
#[derive(Clone)]
enum MockTokenDataKey {
    Balance(Address),
    TotalSupply,
    TotalAssets,
}

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn vault_mint(env: Env, to: Address, shares: i128) {
        let balance_key = MockTokenDataKey::Balance(to.clone());
        let balance = env
            .storage()
            .instance()
            .get::<_, i128>(&balance_key)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&balance_key, &(balance + shares));
        let total_supply = env
            .storage()
            .instance()
            .get::<_, i128>(&MockTokenDataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&MockTokenDataKey::TotalSupply, &(total_supply + shares));
    }

    pub fn bridge_mint(env: Env, to: Address, shares: i128) {
        Self::vault_mint(env, to, shares);
    }

    pub fn vault_burn(env: Env, from: Address, shares: i128) {
        let balance_key = MockTokenDataKey::Balance(from);
        let balance = env
            .storage()
            .instance()
            .get::<_, i128>(&balance_key)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&balance_key, &(balance - shares));
        let total_supply = env
            .storage()
            .instance()
            .get::<_, i128>(&MockTokenDataKey::TotalSupply)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&MockTokenDataKey::TotalSupply, &(total_supply - shares));
    }

    pub fn bridge_burn(env: Env, from: Address, shares: i128) {
        Self::vault_burn(env, from, shares);
    }

    pub fn update_exchange_rate(env: Env, new_total_assets: i128) {
        env.storage()
            .instance()
            .set(&MockTokenDataKey::TotalAssets, &new_total_assets);
    }

    pub fn balance(env: Env, user: Address) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&MockTokenDataKey::Balance(user))
            .unwrap_or(0)
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&MockTokenDataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn total_assets(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&MockTokenDataKey::TotalAssets)
            .unwrap_or(0)
    }
}

struct BridgeHandlerTestFixture<'a> {
    env: Env,
    client: BridgeHandlerClient<'a>,
    gateway: MockGatewayClient<'a>,
    gas_service: MockGasServiceClient<'a>,
    token: MockTokenClient<'a>,
    admin: Address,
    vault: Address,
    user: Address,
    source_chain: String,
    source_address: String,
}

impl<'a> BridgeHandlerTestFixture<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.ledger().set(LedgerInfo {
            timestamp: 1_700_000_000,
            protocol_version: 22,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: TTL_BUMP + 1_000,
        });
        env.mock_all_auths_allowing_non_root_auth();

        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let user = Address::generate(&env);
        let source_chain = String::from_str(&env, "ethereum");
        let source_address = String::from_str(&env, "0xHelixVault");

        let gateway_id = env.register(MockGateway, ());
        let gas_service_id = env.register(MockGasService, ());
        let token_id = env.register(MockToken, ());
        let handler_id = env.register(
            BridgeHandler,
            (
                admin.clone(),
                gateway_id.clone(),
                gas_service_id.clone(),
                token_id.clone(),
                vault.clone(),
                source_chain.clone(),
                source_address.clone(),
            ),
        );

        let gateway = MockGatewayClient::new(&env, &gateway_id);
        gateway.set_approved(&true);

        Self {
            env: env.clone(),
            client: BridgeHandlerClient::new(&env, &handler_id),
            gateway,
            gas_service: MockGasServiceClient::new(&env, &gas_service_id),
            token: MockTokenClient::new(&env, &token_id),
            admin,
            vault,
            user,
            source_chain,
            source_address,
        }
    }

    fn deposit_payload(&self, recipient: &Address, shares: i128, nonce: u64) -> Bytes {
        let recipient_bytes = contract_bytes32(&self.env, recipient);
        self.deposit_payload_for_bytes32(&recipient_bytes, shares, nonce)
    }

    fn deposit_payload_for_bytes32(
        &self,
        recipient: &BytesN<32>,
        shares: i128,
        nonce: u64,
    ) -> Bytes {
        Bytes::from_slice(
            &self.env,
            &DepositMessage {
                messageType: MESSAGE_TYPE_DEPOSIT,
                stellarRecipient: FixedBytes::<32>::new(recipient.to_array()),
                shares: U256::from(shares as u128),
                nonce: U256::from(nonce),
            }
            .abi_encode_params(),
        )
    }

    fn yield_payload(&self, exchange_rate: i128, timestamp: u64, nonce: u64) -> Bytes {
        Bytes::from_slice(
            &self.env,
            &super::YieldUpdateMessage {
                messageType: MESSAGE_TYPE_YIELD_UPDATE,
                exchangeRate: U256::from(exchange_rate as u128),
                timestamp: U256::from(timestamp),
                nonce: U256::from(nonce),
            }
            .abi_encode_params(),
        )
    }

    fn eth_recipient(&self) -> BytesN<32> {
        let mut bytes = [0_u8; 32];
        bytes[12..].copy_from_slice(&[
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, 0x00, 0x12, 0x34, 0x56, 0x78,
        ]);
        BytesN::from_array(&self.env, &bytes)
    }
}

fn contract_bytes32(env: &Env, address: &Address) -> BytesN<32> {
    let sc_address: ScAddress = address.clone().into();
    let ScAddress::Contract(Hash(contract_id)) = sc_address else {
        panic!("expected generated address to be a contract address")
    };
    BytesN::from_array(env, &contract_id)
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

fn assert_contract_error<F>(f: F, code: u32)
where
    F: FnOnce(),
{
    let panic = catch_unwind(AssertUnwindSafe(f)).expect_err("expected contract panic");
    let message = if let Some(message) = panic.downcast_ref::<StdString>() {
        message.clone()
    } else if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else {
        format!("{panic:?}")
    };

    assert!(
        message.contains(&format!("Error(Contract, #{code})")),
        "unexpected panic: {message}"
    );
}

#[test]
fn test_constructor_stores_config() {
    let fixture = BridgeHandlerTestFixture::new();

    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Admin),
        fixture.admin
    );
    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::Gateway),
        fixture.gateway.address
    );
    assert_eq!(
        instance_value::<Address>(&fixture.env, &fixture.client.address, &DataKey::GasService),
        fixture.gas_service.address
    );
    assert_eq!(
        instance_value::<Address>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::TokenContract
        ),
        fixture.token.address
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
        instance_value::<String>(&fixture.env, &fixture.client.address, &DataKey::SourceChain),
        fixture.source_chain
    );
    assert_eq!(
        instance_value::<String>(
            &fixture.env,
            &fixture.client.address,
            &DataKey::SourceAddress
        ),
        fixture.source_address
    );
    assert!(!instance_value::<bool>(
        &fixture.env,
        &fixture.client.address,
        &DataKey::Paused
    ));
}

#[test]
fn test_deposit_without_mapping_uses_direct_decode() {
    let fixture = BridgeHandlerTestFixture::new();
    let payload = fixture.deposit_payload(&fixture.user, 150, 1);

    fixture.client.execute(
        &fixture.source_chain,
        &String::from_str(&fixture.env, "deposit-1"),
        &fixture.source_address,
        &payload,
    );
    assert_eq!(fixture.token.balance(&fixture.user), 150);
    assert_eq!(fixture.token.total_supply(), 150);
}

#[test]
fn test_register_and_resolve_recipient() {
    let fixture = BridgeHandlerTestFixture::new();
    let evm_address = fixture.eth_recipient();

    fixture
        .client
        .register_recipient(&fixture.user, &evm_address);
    assert_eq!(
        fixture.client.get_recipient(&evm_address),
        Some(fixture.user.clone())
    );

    let payload = fixture.deposit_payload_for_bytes32(&evm_address, 175, 2);
    fixture.client.execute(
        &fixture.source_chain,
        &String::from_str(&fixture.env, "deposit-mapped"),
        &fixture.source_address,
        &payload,
    );

    assert_eq!(fixture.token.balance(&fixture.user), 175);
    assert_eq!(fixture.token.total_supply(), 175);
}

#[test]
#[should_panic]
fn test_register_recipient_requires_auth() {
    let fixture = BridgeHandlerTestFixture::new();
    fixture.env.set_auths(&[]);

    fixture
        .client
        .register_recipient(&fixture.user, &fixture.eth_recipient());
}

#[test]
fn test_deposit_replay_rejected() {
    let fixture = BridgeHandlerTestFixture::new();
    let payload = fixture.deposit_payload(&fixture.user, 100, 42);

    fixture.client.execute(
        &fixture.source_chain,
        &String::from_str(&fixture.env, "deposit-replay"),
        &fixture.source_address,
        &payload,
    );
    assert_contract_error(
        || {
            fixture.client.execute(
                &fixture.source_chain,
                &String::from_str(&fixture.env, "deposit-replay-2"),
                &fixture.source_address,
                &payload,
            );
        },
        6,
    );
}

#[test]
fn test_deposit_rate_limit_exceeded() {
    let fixture = BridgeHandlerTestFixture::new();
    fixture.client.set_rate_limits(&50, &1_000, &100);

    let payload = fixture.deposit_payload(&fixture.user, 75, 7);
    assert_contract_error(
        || {
            fixture.client.execute(
                &fixture.source_chain,
                &String::from_str(&fixture.env, "deposit-cap"),
                &fixture.source_address,
                &payload,
            );
        },
        7,
    );
}

#[test]
fn test_deposit_invalid_source_chain() {
    let fixture = BridgeHandlerTestFixture::new();
    let payload = fixture.deposit_payload(&fixture.user, 50, 9);

    assert_contract_error(
        || {
            fixture.client.execute(
                &String::from_str(&fixture.env, "arbitrum"),
                &String::from_str(&fixture.env, "deposit-bad-chain"),
                &fixture.source_address,
                &payload,
            );
        },
        4,
    );
}

#[test]
fn test_deposit_invalid_source_address() {
    let fixture = BridgeHandlerTestFixture::new();
    let payload = fixture.deposit_payload(&fixture.user, 50, 11);

    assert_contract_error(
        || {
            fixture.client.execute(
                &fixture.source_chain,
                &String::from_str(&fixture.env, "deposit-bad-address"),
                &String::from_str(&fixture.env, "0xOtherVault"),
                &payload,
            );
        },
        5,
    );
}

#[test]
fn test_withdrawal_burns_and_sends_gmp() {
    let fixture = BridgeHandlerTestFixture::new();
    fixture.token.bridge_mint(&fixture.user, &200);

    let eth_recipient = fixture.eth_recipient();
    fixture
        .client
        .initiate_withdrawal(&fixture.user, &75, &eth_recipient);

    assert_eq!(fixture.token.balance(&fixture.user), 125);
    assert_eq!(
        fixture.gateway.last_destination_chain(),
        Some(fixture.source_chain.clone())
    );
    assert_eq!(
        fixture.gateway.last_destination_address(),
        Some(fixture.source_address.clone())
    );
    assert!(fixture.gas_service.gas_payload().is_some());

    let payload = fixture
        .gateway
        .gateway_payload()
        .expect("gateway payload must exist");
    let decoded = super::WithdrawMessage::abi_decode_params(&payload.to_alloc_vec(), true)
        .expect("withdraw payload must decode");
    assert_eq!(decoded.messageType, MESSAGE_TYPE_WITHDRAW);
    assert_eq!(
        decoded.ethRecipient,
        FixedBytes::<32>::new(eth_recipient.to_array())
    );
    assert_eq!(decoded.shares, U256::from(75_u32));
}

#[test]
fn test_yield_update() {
    let fixture = BridgeHandlerTestFixture::new();
    let payload = fixture.yield_payload(1_500_000_000_000_000_000, 1_700_000_000, 3);

    fixture.client.execute(
        &fixture.source_chain,
        &String::from_str(&fixture.env, "yield-1"),
        &fixture.source_address,
        &payload,
    );
    assert_eq!(fixture.token.total_assets(), 1_500_000_000_000_000_000);
}

#[test]
fn test_pause_blocks_operations() {
    let fixture = BridgeHandlerTestFixture::new();
    fixture.client.pause();

    let payload = fixture.deposit_payload(&fixture.user, 25, 13);
    assert_contract_error(
        || {
            fixture.client.execute(
                &fixture.source_chain,
                &String::from_str(&fixture.env, "deposit-paused"),
                &fixture.source_address,
                &payload,
            );
        },
        3,
    );
    assert_contract_error(
        || {
            fixture
                .client
                .initiate_withdrawal(&fixture.user, &25, &fixture.eth_recipient());
        },
        3,
    );
}

#[test]
fn test_pause_unpause() {
    let fixture = BridgeHandlerTestFixture::new();
    fixture.client.pause();
    fixture.client.unpause();

    let payload = fixture.deposit_payload(&fixture.user, 40, 14);
    fixture.client.execute(
        &fixture.source_chain,
        &String::from_str(&fixture.env, "deposit-unpaused"),
        &fixture.source_address,
        &payload,
    );
    assert_eq!(fixture.token.balance(&fixture.user), 40);
}
