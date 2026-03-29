#![no_std]
#![allow(clippy::too_many_arguments)]

extern crate alloc;

use alloc::string::String as StdString;
use alloy_primitives::{FixedBytes, Uint, U256};
use alloy_sol_types::{sol, SolValue};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, vec,
    Address, Bytes, BytesN, Env, IntoVal, String, Symbol, TryFromVal, Val, Vec,
};

const TTL_THRESHOLD: u32 = 17_280;
const TTL_BUMP: u32 = 518_400;
const DEFAULT_EPOCH_DURATION: u32 = 17_280;
const DEFAULT_GAS_AMOUNT: i128 = 10_000_000_000;
const GAS_TOKEN_ADDRESS: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
const MESSAGE_TYPE_DEPOSIT: u8 = 0x01;
const MESSAGE_TYPE_WITHDRAW: u8 = 0x02;
const MESSAGE_TYPE_YIELD_UPDATE: u8 = 0x03;
const STRKEY_CONTRACT_VERSION_BYTE: u8 = 2 << 3;
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

sol! {
    struct DepositMessage {
        uint8 messageType;
        bytes32 stellarRecipient;
        uint256 shares;
        uint256 nonce;
    }

    struct WithdrawMessage {
        uint8 messageType;
        bytes32 ethRecipient;
        uint256 shares;
        uint256 nonce;
    }

    struct YieldUpdateMessage {
        uint8 messageType;
        uint256 exchangeRate;
        uint256 timestamp;
        uint256 nonce;
    }
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Gateway,
    GasService,
    TokenContract,
    VaultContract,
    SourceChain,
    SourceAddress,
    Paused,
    MaxMintPerEpoch,
    MaxBurnPerEpoch,
    EpochDuration,
    ProcessedMessage(BytesN<32>),
    RecipientMapping(BytesN<32>),
    EpochMints(u64),
    EpochBurns(u64),
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
pub enum BridgeHandlerError {
    NotInitialized = 1,
    Unauthorized = 2,
    Paused = 3,
    InvalidSourceChain = 4,
    InvalidSourceAddress = 5,
    MessageAlreadyProcessed = 6,
    RateLimitExceeded = 7,
    InvalidPayload = 8,
    InvalidMessageType = 9,
    InvalidAmount = 10,
    AbiDecodeFailed = 11,
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

#[contractclient(name = "HelixTokenClient")]
pub trait HelixTokenInterface {
    fn vault_mint(env: Env, to: Address, shares: i128);
    fn vault_burn(env: Env, from: Address, shares: i128);
    fn bridge_mint(env: Env, to: Address, shares: i128);
    fn bridge_burn(env: Env, from: Address, shares: i128);
    fn update_exchange_rate(env: Env, new_total_assets: i128);
}

#[contract]
pub struct BridgeHandler;

#[contractimpl]
impl BridgeHandler {
    #[allow(clippy::too_many_arguments)]
    pub fn __constructor(
        env: Env,
        admin: Address,
        gateway: Address,
        gas_service: Address,
        token: Address,
        vault: Address,
        source_chain: String,
        source_address: String,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Gateway, &gateway);
        env.storage()
            .instance()
            .set(&DataKey::GasService, &gas_service);
        env.storage()
            .instance()
            .set(&DataKey::TokenContract, &token);
        env.storage()
            .instance()
            .set(&DataKey::VaultContract, &vault);
        env.storage()
            .instance()
            .set(&DataKey::SourceChain, &source_chain);
        env.storage()
            .instance()
            .set(&DataKey::SourceAddress, &source_address);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::MaxMintPerEpoch, &i128::MAX);
        env.storage()
            .instance()
            .set(&DataKey::MaxBurnPerEpoch, &i128::MAX);
        env.storage()
            .instance()
            .set(&DataKey::EpochDuration, &DEFAULT_EPOCH_DURATION);
        Self::extend_instance(&env);
    }

    pub fn execute(
        env: Env,
        source_chain: String,
        message_id: String,
        source_address: String,
        payload: Bytes,
    ) -> Result<(), BridgeHandlerError> {
        Self::require_initialized(&env);
        Self::extend_instance(&env);

        if source_chain != Self::source_chain(&env) {
            return Err(BridgeHandlerError::InvalidSourceChain);
        }
        if source_address != Self::source_address(&env) {
            return Err(BridgeHandlerError::InvalidSourceAddress);
        }

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
            return Err(BridgeHandlerError::Unauthorized);
        }

        match Self::get_message_type(&payload)? {
            MESSAGE_TYPE_DEPOSIT => {
                Self::process_deposit(&env, &message_id, &payload, &payload_hash)
            }
            MESSAGE_TYPE_YIELD_UPDATE => {
                Self::process_yield_update(&env, &message_id, &payload, &payload_hash)
            }
            _ => Err(BridgeHandlerError::InvalidMessageType),
        }
    }

    pub fn register_recipient(env: Env, user: Address, evm_address: BytesN<32>) {
        Self::require_initialized(&env);
        user.require_auth();
        Self::extend_instance(&env);
        Self::write_recipient_mapping(&env, &evm_address, &user);
        env.events().publish(
            (Symbol::new(&env, "recipient_registered"), user),
            (evm_address,),
        );
    }

    pub fn get_recipient(env: Env, evm_address: BytesN<32>) -> Option<Address> {
        Self::require_initialized(&env);
        Self::read_recipient_mapping(&env, &evm_address)
    }

    pub fn initiate_withdrawal(
        env: Env,
        user: Address,
        shares: i128,
        eth_recipient: BytesN<32>,
    ) -> Result<(), BridgeHandlerError> {
        Self::require_initialized(&env);
        Self::extend_instance(&env);
        user.require_auth();
        Self::require_not_paused(&env)?;
        Self::validate_amount(shares)?;

        let epoch = Self::current_epoch(&env)?;
        let current_burns = Self::read_epoch_burns(&env, epoch);
        let updated_burns = Self::checked_add(current_burns, shares)?;
        if updated_burns > Self::max_burn_per_epoch(&env) {
            return Err(BridgeHandlerError::RateLimitExceeded);
        }

        let token_address = Self::token_contract(&env);
        Self::authorize_current_contract_call(
            &env,
            &token_address,
            "bridge_burn",
            vec![&env, user.clone().into_val(&env), shares.into_val(&env)],
        );
        HelixTokenClient::new(&env, &token_address).bridge_burn(&user, &shares);

        let nonce = Self::withdrawal_nonce(&env, current_burns);
        let payload = Self::encode_withdraw_message(&env, &eth_recipient, shares, nonce)?;
        let sender = env.current_contract_address();
        let destination_chain = Self::source_chain(&env);
        let destination_address = Self::source_address(&env);
        let gas_token = Self::default_gas_token(&env);

        Self::extend_instance(&env);
        AxelarGasServiceClient::new(&env, &Self::gas_service(&env)).pay_gas(
            &sender,
            &destination_chain,
            &destination_address,
            &payload,
            &user,
            &gas_token,
            &Bytes::new(&env),
        );
        AxelarGatewayMessagingClient::new(&env, &Self::gateway(&env)).call_contract(
            &sender,
            &destination_chain,
            &destination_address,
            &payload,
        );

        Self::write_epoch_burns(&env, epoch, updated_burns);
        let payload_hash: BytesN<32> = env.crypto().keccak256(&payload).into();
        env.events().publish(
            (Symbol::new(&env, "withdrawal_initiated"), user),
            (shares, eth_recipient, payload_hash),
        );
        Ok(())
    }

    pub fn set_rate_limits(
        env: Env,
        max_mint: i128,
        max_burn: i128,
        epoch_duration: u32,
    ) -> Result<(), BridgeHandlerError> {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        if max_mint < 0 || max_burn < 0 || epoch_duration == 0 {
            return Err(BridgeHandlerError::InvalidAmount);
        }

        Self::extend_instance(&env);
        env.storage()
            .instance()
            .set(&DataKey::MaxMintPerEpoch, &max_mint);
        env.storage()
            .instance()
            .set(&DataKey::MaxBurnPerEpoch, &max_burn);
        env.storage()
            .instance()
            .set(&DataKey::EpochDuration, &epoch_duration);
        Ok(())
    }

    pub fn pause(env: Env) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        Self::extend_instance(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    pub fn unpause(env: Env) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        Self::extend_instance(&env);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        Self::extend_instance(&env);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        Self::require_initialized(&env);
        Self::admin(&env).require_auth();
        Self::extend_instance(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

impl BridgeHandler {
    fn process_deposit(
        env: &Env,
        message_id: &String,
        payload: &Bytes,
        payload_hash: &BytesN<32>,
    ) -> Result<(), BridgeHandlerError> {
        let decoded = Self::decode_deposit_message(payload)?;
        Self::require_not_paused(env)?;
        Self::require_message_not_processed(env, payload_hash)?;

        let shares = Self::u256_to_i128(decoded.shares)?;
        let epoch = Self::current_epoch(env)?;
        let current_mints = Self::read_epoch_mints(env, epoch);
        let updated_mints = Self::checked_add(current_mints, shares)?;
        if updated_mints > Self::max_mint_per_epoch(env) {
            return Err(BridgeHandlerError::RateLimitExceeded);
        }

        let recipient_bytes = decoded.stellarRecipient.0;
        let evm_key = DataKey::RecipientMapping(BytesN::from_array(env, &recipient_bytes));
        let recipient = if env.storage().persistent().has(&evm_key) {
            env.storage()
                .persistent()
                .get::<_, Address>(&evm_key)
                .unwrap()
        } else {
            Self::bytes32_to_address(env, recipient_bytes)?
        };
        let token_address = Self::token_contract(env);
        Self::authorize_current_contract_call(
            env,
            &token_address,
            "bridge_mint",
            vec![env, recipient.clone().into_val(env), shares.into_val(env)],
        );
        HelixTokenClient::new(env, &token_address).bridge_mint(&recipient, &shares);

        Self::write_processed_message(env, payload_hash);
        Self::write_epoch_mints(env, epoch, updated_mints);
        env.events().publish(
            (Symbol::new(env, "deposit_processed"), recipient),
            (shares, message_id.clone(), payload_hash.clone()),
        );
        Ok(())
    }

    fn process_yield_update(
        env: &Env,
        message_id: &String,
        payload: &Bytes,
        payload_hash: &BytesN<32>,
    ) -> Result<(), BridgeHandlerError> {
        let decoded = Self::decode_yield_update_message(payload)?;
        Self::require_not_paused(env)?;
        Self::require_message_not_processed(env, payload_hash)?;

        let new_total_assets = Self::u256_to_i128(decoded.exchangeRate)?;
        let token_address = Self::token_contract(env);
        Self::authorize_current_contract_call(
            env,
            &token_address,
            "update_exchange_rate",
            vec![env, new_total_assets.into_val(env)],
        );
        HelixTokenClient::new(env, &token_address).update_exchange_rate(&new_total_assets);

        Self::write_processed_message(env, payload_hash);
        env.events().publish(
            (Symbol::new(env, "yield_updated"),),
            (new_total_assets, message_id.clone(), payload_hash.clone()),
        );
        Ok(())
    }

    fn decode_deposit_message(payload: &Bytes) -> Result<DepositMessage, BridgeHandlerError> {
        DepositMessage::abi_decode_params(&payload.to_alloc_vec(), true)
            .map_err(|_| BridgeHandlerError::AbiDecodeFailed)
    }

    fn decode_yield_update_message(
        payload: &Bytes,
    ) -> Result<YieldUpdateMessage, BridgeHandlerError> {
        YieldUpdateMessage::abi_decode_params(&payload.to_alloc_vec(), true)
            .map_err(|_| BridgeHandlerError::AbiDecodeFailed)
    }

    fn encode_withdraw_message(
        env: &Env,
        eth_recipient: &BytesN<32>,
        shares: i128,
        nonce: u128,
    ) -> Result<Bytes, BridgeHandlerError> {
        Ok(Bytes::from_slice(
            env,
            &WithdrawMessage {
                messageType: MESSAGE_TYPE_WITHDRAW,
                ethRecipient: FixedBytes::<32>::new(eth_recipient.to_array()),
                shares: Self::i128_to_u256(shares)?,
                nonce: U256::from(nonce),
            }
            .abi_encode_params(),
        ))
    }

    fn get_message_type(payload: &Bytes) -> Result<u8, BridgeHandlerError> {
        if payload.len() < 32 {
            return Err(BridgeHandlerError::InvalidPayload);
        }
        Ok(payload.slice(0..32).last_unchecked())
    }

    fn bytes32_to_address(env: &Env, contract_id: [u8; 32]) -> Result<Address, BridgeHandlerError> {
        let strkey = Self::contract_id_to_strkey(contract_id);
        Ok(Address::from_str(env, &strkey))
    }

    fn contract_id_to_strkey(contract_id: [u8; 32]) -> StdString {
        let mut data = [0_u8; 35];
        data[0] = STRKEY_CONTRACT_VERSION_BYTE;
        data[1..33].copy_from_slice(&contract_id);
        let checksum = Self::crc16_xmodem(&data[..33]);
        data[33..35].copy_from_slice(&checksum);
        Self::base32_encode_no_pad(&data)
    }

    fn base32_encode_no_pad(data: &[u8]) -> StdString {
        let output_len = (data.len() * 8).div_ceil(5);
        let mut output = StdString::with_capacity(output_len);
        let mut buffer: u32 = 0;
        let mut bits_left: u8 = 0;

        for &byte in data {
            buffer = (buffer << 8) | byte as u32;
            bits_left += 8;

            while bits_left >= 5 {
                let index = ((buffer >> (bits_left - 5)) & 0x1f) as usize;
                output.push(BASE32_ALPHABET[index] as char);
                bits_left -= 5;
            }
        }

        if bits_left > 0 {
            let index = ((buffer << (5 - bits_left)) & 0x1f) as usize;
            output.push(BASE32_ALPHABET[index] as char);
        }

        output
    }

    fn crc16_xmodem(data: &[u8]) -> [u8; 2] {
        let mut crc: u16 = 0;
        for byte in data.iter() {
            crc = (crc << 8) ^ CRC16_TABLE[((crc >> 8) as u8 ^ *byte) as usize];
        }
        [(crc & 0xff) as u8, (crc >> 8) as u8]
    }

    fn require_initialized(env: &Env) {
        if !env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, BridgeHandlerError::NotInitialized);
        }
    }

    fn require_not_paused(env: &Env) -> Result<(), BridgeHandlerError> {
        if Self::paused(env) {
            return Err(BridgeHandlerError::Paused);
        }
        Ok(())
    }

    fn require_message_not_processed(
        env: &Env,
        payload_hash: &BytesN<32>,
    ) -> Result<(), BridgeHandlerError> {
        if env
            .storage()
            .persistent()
            .has(&DataKey::ProcessedMessage(payload_hash.clone()))
        {
            return Err(BridgeHandlerError::MessageAlreadyProcessed);
        }
        Ok(())
    }

    fn validate_amount(amount: i128) -> Result<(), BridgeHandlerError> {
        if amount < 0 {
            return Err(BridgeHandlerError::InvalidAmount);
        }
        Ok(())
    }

    fn current_epoch(env: &Env) -> Result<u64, BridgeHandlerError> {
        let epoch_duration = Self::epoch_duration(env);
        if epoch_duration == 0 {
            return Err(BridgeHandlerError::InvalidAmount);
        }
        Ok(env.ledger().sequence() as u64 / epoch_duration as u64)
    }

    fn withdrawal_nonce(env: &Env, current_burns: i128) -> u128 {
        ((env.ledger().sequence() as u128) << 64)
            | u128::from(u64::try_from(current_burns.max(0)).unwrap_or(u64::MAX))
    }

    fn default_gas_token(env: &Env) -> GasToken {
        GasToken {
            address: Address::from_str(env, GAS_TOKEN_ADDRESS),
            amount: DEFAULT_GAS_AMOUNT,
        }
    }

    fn u256_to_i128(value: Uint<256, 4>) -> Result<i128, BridgeHandlerError> {
        let slice = value.as_le_slice();
        let mut high_bytes = [0_u8; 16];
        let mut low_bytes = [0_u8; 16];
        low_bytes.copy_from_slice(&slice[..16]);
        high_bytes.copy_from_slice(&slice[16..]);

        if i128::from_le_bytes(high_bytes) != 0 {
            return Err(BridgeHandlerError::InvalidAmount);
        }

        let decoded = i128::from_le_bytes(low_bytes);
        if decoded < 0 {
            return Err(BridgeHandlerError::InvalidAmount);
        }

        Ok(decoded)
    }

    fn i128_to_u256(value: i128) -> Result<U256, BridgeHandlerError> {
        let value = u128::try_from(value).map_err(|_| BridgeHandlerError::InvalidAmount)?;
        Ok(U256::from(value))
    }

    fn checked_add(left: i128, right: i128) -> Result<i128, BridgeHandlerError> {
        left.checked_add(right)
            .ok_or(BridgeHandlerError::InvalidAmount)
    }

    fn get_instance<T>(env: &Env, key: &DataKey) -> T
    where
        T: TryFromVal<Env, Val>,
    {
        match env.storage().instance().get::<_, T>(key) {
            Some(value) => value,
            None => panic_with_error!(env, BridgeHandlerError::NotInitialized),
        }
    }

    fn admin(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Admin)
    }

    fn gateway(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::Gateway)
    }

    fn gas_service(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::GasService)
    }

    fn token_contract(env: &Env) -> Address {
        Self::get_instance(env, &DataKey::TokenContract)
    }

    fn source_chain(env: &Env) -> String {
        Self::get_instance(env, &DataKey::SourceChain)
    }

    fn source_address(env: &Env) -> String {
        Self::get_instance(env, &DataKey::SourceAddress)
    }

    fn paused(env: &Env) -> bool {
        Self::get_instance(env, &DataKey::Paused)
    }

    fn max_mint_per_epoch(env: &Env) -> i128 {
        Self::get_instance(env, &DataKey::MaxMintPerEpoch)
    }

    fn max_burn_per_epoch(env: &Env) -> i128 {
        Self::get_instance(env, &DataKey::MaxBurnPerEpoch)
    }

    fn epoch_duration(env: &Env) -> u32 {
        Self::get_instance(env, &DataKey::EpochDuration)
    }

    fn read_epoch_mints(env: &Env, epoch: u64) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::EpochMints(epoch))
            .unwrap_or(0)
    }

    fn write_epoch_mints(env: &Env, epoch: u64, amount: i128) {
        let key = DataKey::EpochMints(epoch);
        env.storage().persistent().set(&key, &amount);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn read_epoch_burns(env: &Env, epoch: u64) -> i128 {
        env.storage()
            .persistent()
            .get::<_, i128>(&DataKey::EpochBurns(epoch))
            .unwrap_or(0)
    }

    fn write_epoch_burns(env: &Env, epoch: u64, amount: i128) {
        let key = DataKey::EpochBurns(epoch);
        env.storage().persistent().set(&key, &amount);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn write_processed_message(env: &Env, payload_hash: &BytesN<32>) {
        let key = DataKey::ProcessedMessage(payload_hash.clone());
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn read_recipient_mapping(env: &Env, evm_address: &BytesN<32>) -> Option<Address> {
        env.storage()
            .persistent()
            .get::<_, Address>(&DataKey::RecipientMapping(evm_address.clone()))
    }

    fn write_recipient_mapping(env: &Env, evm_address: &BytesN<32>, user: &Address) {
        let key = DataKey::RecipientMapping(evm_address.clone());
        env.storage().persistent().set(&key, user);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_BUMP);
    }

    fn authorize_current_contract_call(
        env: &Env,
        contract: &Address,
        fn_name: &str,
        args: Vec<Val>,
    ) {
        env.authorize_as_current_contract(vec![
            env,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: contract.clone(),
                    fn_name: Symbol::new(env, fn_name),
                    args,
                },
                sub_invocations: vec![env],
            }),
        ]);
    }

    fn extend_instance(env: &Env) {
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_BUMP);
    }
}

const CRC16_TABLE: [u16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7, 0x8108, 0x9129, 0xa14a, 0xb16b,
    0xc18c, 0xd1ad, 0xe1ce, 0xf1ef, 0x1231, 0x0210, 0x3273, 0x2252, 0x52b5, 0x4294, 0x72f7, 0x62d6,
    0x9339, 0x8318, 0xb37b, 0xa35a, 0xd3bd, 0xc39c, 0xf3ff, 0xe3de, 0x2462, 0x3443, 0x0420, 0x1401,
    0x64e6, 0x74c7, 0x44a4, 0x5485, 0xa56a, 0xb54b, 0x8528, 0x9509, 0xe5ee, 0xf5cf, 0xc5ac, 0xd58d,
    0x3653, 0x2672, 0x1611, 0x0630, 0x76d7, 0x66f6, 0x5695, 0x46b4, 0xb75b, 0xa77a, 0x9719, 0x8738,
    0xf7df, 0xe7fe, 0xd79d, 0xc7bc, 0x48c4, 0x58e5, 0x6886, 0x78a7, 0x0840, 0x1861, 0x2802, 0x3823,
    0xc9cc, 0xd9ed, 0xe98e, 0xf9af, 0x8948, 0x9969, 0xa90a, 0xb92b, 0x5af5, 0x4ad4, 0x7ab7, 0x6a96,
    0x1a71, 0x0a50, 0x3a33, 0x2a12, 0xdbfd, 0xcbdc, 0xfbbf, 0xeb9e, 0x9b79, 0x8b58, 0xbb3b, 0xab1a,
    0x6ca6, 0x7c87, 0x4ce4, 0x5cc5, 0x2c22, 0x3c03, 0x0c60, 0x1c41, 0xedae, 0xfd8f, 0xcdec, 0xddcd,
    0xad2a, 0xbd0b, 0x8d68, 0x9d49, 0x7e97, 0x6eb6, 0x5ed5, 0x4ef4, 0x3e13, 0x2e32, 0x1e51, 0x0e70,
    0xff9f, 0xefbe, 0xdfdd, 0xcffc, 0xbf1b, 0xaf3a, 0x9f59, 0x8f78, 0x9188, 0x81a9, 0xb1ca, 0xa1eb,
    0xd10c, 0xc12d, 0xf14e, 0xe16f, 0x1080, 0x00a1, 0x30c2, 0x20e3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83b9, 0x9398, 0xa3fb, 0xb3da, 0xc33d, 0xd31c, 0xe37f, 0xf35e, 0x02b1, 0x1290, 0x22f3, 0x32d2,
    0x4235, 0x5214, 0x6277, 0x7256, 0xb5ea, 0xa5cb, 0x95a8, 0x8589, 0xf56e, 0xe54f, 0xd52c, 0xc50d,
    0x34e2, 0x24c3, 0x14a0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405, 0xa7db, 0xb7fa, 0x8799, 0x97b8,
    0xe75f, 0xf77e, 0xc71d, 0xd73c, 0x26d3, 0x36f2, 0x0691, 0x16b0, 0x6657, 0x7676, 0x4615, 0x5634,
    0xd94c, 0xc96d, 0xf90e, 0xe92f, 0x99c8, 0x89e9, 0xb98a, 0xa9ab, 0x5844, 0x4865, 0x7806, 0x6827,
    0x18c0, 0x08e1, 0x3882, 0x28a3, 0xcb7d, 0xdb5c, 0xeb3f, 0xfb1e, 0x8bf9, 0x9bd8, 0xabbb, 0xbb9a,
    0x4a75, 0x5a54, 0x6a37, 0x7a16, 0x0af1, 0x1ad0, 0x2ab3, 0x3a92, 0xfd2e, 0xed0f, 0xdd6c, 0xcd4d,
    0xbdaa, 0xad8b, 0x9de8, 0x8dc9, 0x7c26, 0x6c07, 0x5c64, 0x4c45, 0x3ca2, 0x2c83, 0x1ce0, 0x0cc1,
    0xef1f, 0xff3e, 0xcf5d, 0xdf7c, 0xaf9b, 0xbfba, 0x8fd9, 0x9ff8, 0x6e17, 0x7e36, 0x4e55, 0x5e74,
    0x2e93, 0x3eb2, 0x0ed1, 0x1ef0,
];

#[cfg(test)]
mod test;
