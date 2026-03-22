#![no_std]

use core::str;

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    Address, Bytes, BytesN, Env, String, Symbol,
};

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;
const MAX_MESSAGE_BYTES: usize = 1024;
const GAS_TOKEN_ADDRESS: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const DEFAULT_GAS_AMOUNT: i128 = 10_000_000_000;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Gateway,
    GasService,
    LastReceivedMessage,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GasToken {
    pub address: Address,
    pub amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BridgePocError {
    NotInitialized = 1,
    EmptyMessage = 2,
    MessageNotApproved = 3,
    InvalidUtf8 = 4,
    MessageTooLong = 5,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum GasServiceError {
    MigrationNotAllowed = 1,
    InvalidAmount = 2,
    InsufficientBalance = 3,
    MigrationInProgress = 4,
}

#[contractclient(name = "AxelarGatewayMessagingClient")]
pub trait AxelarGatewayMessagingInterface {
    fn call_contract(
        env: Env,
        caller: Address,
        destination_chain: String,
        destination_address: String,
        payload: Bytes,
    );

    fn validate_message(
        env: Env,
        caller: Address,
        source_chain: String,
        message_id: String,
        source_address: String,
        payload_hash: BytesN<32>,
    ) -> bool;
}

#[allow(clippy::too_many_arguments)]
#[contractclient(name = "AxelarGasServiceClient")]
pub trait AxelarGasServiceInterface {
    #[allow(clippy::too_many_arguments)]
    fn pay_gas(
        env: Env,
        sender: Address,
        destination_chain: String,
        destination_address: String,
        payload: Bytes,
        spender: Address,
        token: GasToken,
        metadata: Bytes,
    ) -> Result<(), GasServiceError>;
}

#[contract]
pub struct BridgePoc;

#[contractimpl]
impl BridgePoc {
    pub fn __constructor(env: Env, gateway: Address, gas_service: Address) {
        env.storage().instance().set(&DataKey::Gateway, &gateway);
        env.storage().instance().set(&DataKey::GasService, &gas_service);
        Self::extend_instance(&env);
    }

    pub fn send_message(
        env: Env,
        caller: Address,
        destination_chain: String,
        destination_address: String,
        message: String,
    ) {
        Self::require_initialized(&env);
        caller.require_auth();

        let payload = Self::message_to_bytes(&env, &message);
        let sender = env.current_contract_address();
        let gateway = AxelarGatewayMessagingClient::new(&env, &Self::gateway(&env));
        let gas_service = AxelarGasServiceClient::new(&env, &Self::gas_service(&env));
        let gas_token = Self::default_gas_token(&env);

        Self::extend_instance(&env);
        gas_service.pay_gas(
            &sender,
            &destination_chain,
            &destination_address,
            &payload,
            &caller,
            &gas_token,
            &Bytes::new(&env),
        );

        gateway.call_contract(&sender, &destination_chain, &destination_address, &payload);

        let payload_hash: BytesN<32> = env.crypto().keccak256(&payload).into();
        env.events().publish(
            (Symbol::new(&env, "message_sent"), caller),
            (destination_chain, destination_address, payload_hash),
        );
    }

    pub fn execute(
        env: Env,
        source_chain: String,
        message_id: String,
        source_address: String,
        payload: Bytes,
    ) -> Result<(), BridgePocError> {
        Self::require_initialized(&env);
        Self::extend_instance(&env);

        let payload_hash: BytesN<32> = env.crypto().keccak256(&payload).into();
        let gateway = AxelarGatewayMessagingClient::new(&env, &Self::gateway(&env));
        let is_approved = gateway.validate_message(
            &env.current_contract_address(),
            &source_chain,
            &message_id,
            &source_address,
            &payload_hash,
        );
        if !is_approved {
            return Err(BridgePocError::MessageNotApproved);
        }

        let received_message = Self::bytes_to_message(&env, &payload)?;
        env.storage()
            .instance()
            .set(&DataKey::LastReceivedMessage, &received_message);
        env.events().publish(
            (Symbol::new(&env, "message_received"),),
            (source_chain, message_id, source_address, payload_hash),
        );

        Ok(())
    }

    pub fn received_message(env: Env) -> Option<String> {
        Self::require_initialized(&env);
        Self::extend_instance(&env);
        env.storage()
            .instance()
            .get::<_, String>(&DataKey::LastReceivedMessage)
    }

    fn gateway(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Gateway)
    }

    fn gas_service(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::GasService)
    }

    fn get_instance<T>(env: &Env, key: &DataKey) -> T
    where
        T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    {
        match env.storage().instance().get::<_, T>(key) {
            Some(value) => value,
            None => panic_with_error!(env, BridgePocError::NotInitialized),
        }
    }

    fn require_initialized(env: &Env) {
        let _: Address = Self::gateway(env);
        let _: Address = Self::gas_service(env);
    }

    fn default_gas_token(env: &Env) -> GasToken {
        GasToken {
            address: Address::from_str(env, GAS_TOKEN_ADDRESS),
            amount: DEFAULT_GAS_AMOUNT,
        }
    }

    fn message_to_bytes(env: &Env, message: &String) -> Bytes {
        if message.is_empty() {
            panic_with_error!(env, BridgePocError::EmptyMessage);
        }

        let length = message.len() as usize;
        if length > MAX_MESSAGE_BYTES {
            panic_with_error!(env, BridgePocError::MessageTooLong);
        }

        let mut buffer = [0_u8; MAX_MESSAGE_BYTES];
        let slice = &mut buffer[..length];
        message.copy_into_slice(slice);
        Bytes::from_slice(env, slice)
    }

    fn bytes_to_message(env: &Env, payload: &Bytes) -> Result<String, BridgePocError> {
        if payload.len() as usize > MAX_MESSAGE_BYTES {
            return Err(BridgePocError::MessageTooLong);
        }

        let bytes = payload.to_buffer::<MAX_MESSAGE_BYTES>();
        let value = str::from_utf8(bytes.as_slice()).map_err(|_| BridgePocError::InvalidUtf8)?;
        Ok(String::from_str(env, value))
    }

    fn extend_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_BUMP);
    }
}

#[cfg(test)]
mod test;
