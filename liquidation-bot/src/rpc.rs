use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Context, Result};
use soroban_client::{
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    soroban_rpc::{EventType, GetEventsResponse},
    transaction::TransactionBehavior,
    transaction_builder::TransactionBuilder,
    transaction_builder::TransactionBuilderBehavior,
    xdr::{Int128Parts, LedgerEntryData, ScMap, ScMapEntry, ScSymbol, ScVal},
    Durability, EventFilter, Options, Pagination, Server, Topic,
};
use tokio::time::{sleep, timeout};
use tracing::{debug, instrument, warn};

use crate::{
    config::Config,
    types::{PoolConfig, Position},
};

const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const RPC_RETRIES: usize = 3;
const TX_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct RpcClient {
    server: Arc<Server>,
    network_passphrase: String,
    liquidator_keypair: Keypair,
}

impl RpcClient {
    pub fn new(config: &Config) -> Result<Self> {
        let server = Server::new(&config.rpc_url, Options::default())
            .context("failed to create RPC client")?;
        let liquidator_keypair = Keypair::from_secret(&config.liquidator_secret_key)
            .map_err(|err| anyhow!("invalid LIQUIDATOR_SECRET_KEY: {err}"))?;

        Ok(Self {
            server: Arc::new(server),
            network_passphrase: config.network_passphrase.clone(),
            liquidator_keypair,
        })
    }

    pub fn liquidator_public_key(&self) -> String {
        self.liquidator_keypair.public_key()
    }

    pub async fn latest_ledger(&self) -> Result<u64> {
        let response = self
            .with_retry("getLatestLedger", || async {
                self.server
                    .get_latest_ledger()
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;
        Ok(u64::from(response.sequence))
    }

    pub async fn get_events(
        &self,
        from_ledger: u64,
        contract_id: &str,
        event_names: &[&str],
        limit: u32,
    ) -> Result<GetEventsResponse> {
        let from_ledger =
            u32::try_from(from_ledger).context("event scan start ledger exceeds u32 range")?;

        self.with_retry("getEvents", || async {
            let filters = event_names
                .iter()
                .map(|event_name| {
                    Ok::<_, anyhow::Error>(
                        EventFilter::new(EventType::All)
                            .contract(contract_id)
                            .topic(vec![Topic::Val(sc_symbol(event_name)?)]),
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            self.server
                .get_events(Pagination::From(from_ledger), filters, limit)
                .await
                .map_err(anyhow::Error::from)
        })
        .await
    }

    pub async fn get_health_factor(&self, vault_contract_id: &str, user: &str) -> Result<i128> {
        let result = self
            .simulate_contract_call(
                vault_contract_id,
                "get_health_factor",
                vec![address_arg(user)?],
            )
            .await?;
        parse_i128(&result).context("failed parsing health factor result")
    }

    pub async fn get_position(
        &self,
        vault_contract_id: &str,
        user: &str,
    ) -> Result<Option<Position>> {
        match self
            .simulate_contract_call(vault_contract_id, "get_position", vec![address_arg(user)?])
            .await
        {
            Ok(result) => parse_position(&result).map(Some),
            Err(error) => {
                let message = error.to_string();
                if message.contains("PositionNotFound") || message.contains("Error(Contract, #7)") {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }

    pub async fn last_price(&self, oracle_contract_id: &str, asset: &str) -> Result<(i128, u64)> {
        let result = self
            .simulate_contract_call(oracle_contract_id, "lastprice", vec![address_arg(asset)?])
            .await?;
        parse_price_tuple(&result).context("failed parsing oracle lastprice result")
    }

    pub async fn token_balance(&self, token_contract_id: &str, user: &str) -> Result<i128> {
        let result = self
            .simulate_contract_call(token_contract_id, "balance", vec![address_arg(user)?])
            .await?;
        parse_i128(&result).context("failed parsing token balance result")
    }

    pub async fn read_supported_asset(&self, vault_contract_id: &str) -> Result<Option<String>> {
        let storage = self
            .read_contract_instance_storage(vault_contract_id)
            .await?;
        let Some(value) = storage_lookup(&storage, "SupportedAssets") else {
            return Ok(None);
        };
        let values = parse_vec(value)?;
        let Some(first) = values.first() else {
            return Ok(None);
        };

        parse_address(first).map(Some)
    }

    pub async fn read_pool_config(&self, vault_contract_id: &str) -> Result<Option<PoolConfig>> {
        let storage = self
            .read_contract_instance_storage(vault_contract_id)
            .await?;
        let Some(value) = storage_lookup(&storage, "PoolConfig") else {
            return Ok(None);
        };

        parse_pool_config(value).map(Some)
    }

    pub async fn liquidate(
        &self,
        config: &Config,
        user: &str,
        repay_amount: i128,
    ) -> Result<LiquidationTxResult> {
        let mut source_account = self
            .with_retry("getAccount", || async {
                self.server
                    .get_account(&self.liquidator_public_key())
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        let contract = Contracts::new(&config.vault_contract_id)
            .map_err(|err| anyhow!("invalid vault contract id: {err}"))?;
        let liquidator_address = Address::new(&self.liquidator_public_key())
            .map_err(|err| anyhow!("invalid liquidator public key: {err}"))?;
        let user_address =
            Address::new(user).map_err(|err| anyhow!("invalid target user address: {err}"))?;
        let mut builder =
            TransactionBuilder::new(&mut source_account, &config.network_passphrase, None);
        builder.fee(config.gas_budget);
        builder
            .set_timeout(30)
            .map_err(|err| anyhow!("failed setting transaction timeout: {err}"))?;
        builder.add_operation(contract.call(
            "liquidate",
            Some(vec![
                liquidator_address
                    .to_sc_val()
                    .map_err(|err| anyhow!("failed encoding liquidator address: {err}"))?,
                user_address
                    .to_sc_val()
                    .map_err(|err| anyhow!("failed encoding user address: {err}"))?,
                sc_i128(repay_amount),
            ]),
        ));
        let tx = builder.build();

        let simulation = self
            .with_retry("simulateTransaction", || async {
                self.server
                    .simulate_transaction(&tx, None)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        let min_resource_fee = simulation
            .min_resource_fee
            .as_deref()
            .unwrap_or("0")
            .parse::<u64>()
            .context("failed parsing simulation min_resource_fee")?;

        let mut prepared = self
            .with_retry("prepareTransaction", || async {
                self.server
                    .prepare_transaction(&tx)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        prepared.sign(std::slice::from_ref(&self.liquidator_keypair));
        let tx_hash = hex::encode(prepared.hash());

        let send_response = self
            .with_retry("sendTransaction", || async {
                self.server
                    .send_transaction(prepared.clone())
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        let hash = send_response.hash.clone();
        let final_response = self
            .with_retry("waitTransaction", || async {
                self.server
                    .wait_transaction(&hash, TX_WAIT_TIMEOUT)
                    .await
                    .map_err(|(err, _)| anyhow::Error::from(err))
            })
            .await?;

        Ok(LiquidationTxResult {
            hash,
            status: format!("{:?}", final_response.status),
            simulated_resource_fee: min_resource_fee,
            hash_surrogate: tx_hash,
        })
    }

    async fn simulate_contract_call(
        &self,
        contract_id: &str,
        method: &str,
        args: Vec<ScVal>,
    ) -> Result<ScVal> {
        let public_key = self.liquidator_public_key();
        let mut source_account = self
            .with_retry("getAccount", || async {
                self.server
                    .get_account(&public_key)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        let contract =
            Contracts::new(contract_id).map_err(|err| anyhow!("invalid contract id: {err}"))?;
        let mut builder =
            TransactionBuilder::new(&mut source_account, &self.network_passphrase, None);
        builder.fee(1000u32);
        builder
            .set_timeout(30)
            .map_err(|err| anyhow!("failed setting simulation timeout: {err}"))?;
        builder.add_operation(contract.call(method, Some(args)));
        let tx = builder.build();

        let simulation = self
            .with_retry("simulateTransaction", || async {
                self.server
                    .simulate_transaction(&tx, None)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        simulation
            .to_result()
            .map(|(value, _auth)| value)
            .ok_or_else(|| anyhow!("simulation returned no result for {method}"))
    }

    async fn read_contract_instance_storage(&self, contract_id: &str) -> Result<ScMap> {
        let key = ScVal::LedgerKeyContractInstance;
        let entry = self
            .with_retry("getContractData", || async {
                self.server
                    .get_contract_data(contract_id, key.clone(), Durability::Persistent)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .await?;

        let data = entry.to_data();
        match data {
            LedgerEntryData::ContractData(contract_data) => match contract_data.val {
                ScVal::ContractInstance(instance) => instance
                    .storage
                    .ok_or_else(|| anyhow!("contract instance storage missing")),
                other => bail!("unexpected contract instance value: {other:?}"),
            },
            other => bail!("unexpected ledger entry data for contract instance: {other:?}"),
        }
    }

    #[instrument(skip_all, fields(method = method))]
    async fn with_retry<F, Fut, T>(&self, method: &'static str, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut delay = Duration::from_millis(250);
        let mut last_error = None;

        for attempt in 1..=RPC_RETRIES {
            debug!(attempt, "starting RPC call");
            match timeout(RPC_TIMEOUT, operation()).await {
                Ok(Ok(value)) => return Ok(value),
                Ok(Err(error)) => {
                    warn!(attempt, error = %error, "RPC call failed");
                    last_error = Some(error);
                }
                Err(_) => {
                    let error = anyhow!("RPC call timed out after {:?}", RPC_TIMEOUT);
                    warn!(attempt, error = %error, "RPC call timed out");
                    last_error = Some(error);
                }
            }

            if attempt < RPC_RETRIES {
                sleep(delay).await;
                delay *= 2;
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("RPC call failed without an error")))
    }
}

#[derive(Debug, Clone)]
pub struct LiquidationTxResult {
    pub hash: String,
    pub status: String,
    pub simulated_resource_fee: u64,
    pub hash_surrogate: String,
}

pub fn parse_event_name(value: &ScVal) -> Result<String> {
    match value {
        ScVal::Symbol(symbol) => Ok(symbol.to_string()),
        other => bail!("unexpected topic value for event name: {other:?}"),
    }
}

pub fn parse_address(value: &ScVal) -> Result<String> {
    Address::from_sc_val(value)
        .map(|address| address.to_string())
        .map_err(|err| anyhow!("failed parsing address from ScVal: {err}"))
}

pub fn parse_i128(value: &ScVal) -> Result<i128> {
    match value {
        ScVal::I128(Int128Parts { hi, lo }) => {
            let bytes = [hi.to_be_bytes(), lo.to_be_bytes()].concat();
            let array: [u8; 16] = bytes
                .try_into()
                .map_err(|_| anyhow!("invalid i128 bytes length"))?;
            Ok(i128::from_be_bytes(array))
        }
        other => bail!("unexpected ScVal for i128: {other:?}"),
    }
}

pub fn parse_u64(value: &ScVal) -> Result<u64> {
    match value {
        ScVal::U64(value) => Ok(*value),
        other => bail!("unexpected ScVal for u64: {other:?}"),
    }
}

pub fn parse_vec(value: &ScVal) -> Result<Vec<ScVal>> {
    match value {
        ScVal::Vec(Some(values)) => Ok(values.iter().cloned().collect()),
        other => bail!("unexpected ScVal for vec: {other:?}"),
    }
}

pub fn parse_position(value: &ScVal) -> Result<Position> {
    let map = match value {
        ScVal::Map(Some(entries)) => entries,
        other => bail!("unexpected ScVal for Position: {other:?}"),
    };

    Ok(Position {
        deposited_shares: parse_i128(
            map_lookup(map, "deposited_shares")
                .ok_or_else(|| anyhow!("missing deposited_shares"))?,
        )?,
        borrowed_amount: parse_i128(
            map_lookup(map, "borrowed_amount").ok_or_else(|| anyhow!("missing borrowed_amount"))?,
        )?,
        last_update: parse_u64(
            map_lookup(map, "last_update").ok_or_else(|| anyhow!("missing last_update"))?,
        )?,
    })
}

pub fn parse_pool_config(value: &ScVal) -> Result<PoolConfig> {
    let map = match value {
        ScVal::Map(Some(entries)) => entries,
        other => bail!("unexpected ScVal for PoolConfig: {other:?}"),
    };

    Ok(PoolConfig {
        max_ltv: parse_u32(map_lookup(map, "max_ltv").ok_or_else(|| anyhow!("missing max_ltv"))?)?,
        liq_threshold: parse_u32(
            map_lookup(map, "liq_threshold").ok_or_else(|| anyhow!("missing liq_threshold"))?,
        )?,
        liq_bonus: parse_u32(
            map_lookup(map, "liq_bonus").ok_or_else(|| anyhow!("missing liq_bonus"))?,
        )?,
        interest_rate: parse_u32(
            map_lookup(map, "interest_rate").ok_or_else(|| anyhow!("missing interest_rate"))?,
        )?,
        min_position: parse_i128(
            map_lookup(map, "min_position").ok_or_else(|| anyhow!("missing min_position"))?,
        )?,
    })
}

pub fn parse_price_tuple(value: &ScVal) -> Result<(i128, u64)> {
    let values = parse_vec(value)?;
    if values.len() != 2 {
        bail!(
            "unexpected tuple length for lastprice result: {}",
            values.len()
        );
    }

    Ok((parse_i128(&values[0])?, parse_u64(&values[1])?))
}

pub fn sc_symbol(name: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(
        ScSymbol::try_from(name.as_bytes().to_vec()).context("invalid symbol")?,
    ))
}

pub fn sc_i128(value: i128) -> ScVal {
    let bytes = value.to_be_bytes();
    let hi = i64::from_be_bytes(bytes[0..8].try_into().expect("i128 high bytes"));
    let lo = u64::from_be_bytes(bytes[8..16].try_into().expect("i128 low bytes"));
    ScVal::I128(Int128Parts { hi, lo })
}

pub fn address_arg(address: &str) -> Result<ScVal> {
    Address::new(address)
        .map_err(|err| anyhow!("invalid address {address}: {err}"))?
        .to_sc_val()
        .map_err(|err| anyhow!("failed encoding address {address}: {err}"))
}

fn storage_lookup<'a>(storage: &'a ScMap, variant: &str) -> Option<&'a ScVal> {
    for entry in storage.0.iter() {
        if let ScVal::Vec(Some(values)) = &entry.key {
            if let Some(ScVal::Symbol(symbol)) = values.first() {
                if symbol.to_string() == variant {
                    return Some(&entry.val);
                }
            }
        }
    }
    None
}

fn map_lookup<'a>(entries: &'a ScMap, field: &str) -> Option<&'a ScVal> {
    for ScMapEntry { key, val } in entries.0.iter() {
        if let ScVal::Symbol(symbol) = key {
            if symbol.to_string() == field {
                return Some(val);
            }
        }
    }
    None
}

fn parse_u32(value: &ScVal) -> Result<u32> {
    match value {
        ScVal::U32(value) => Ok(*value),
        other => bail!("unexpected ScVal for u32: {other:?}"),
    }
}
