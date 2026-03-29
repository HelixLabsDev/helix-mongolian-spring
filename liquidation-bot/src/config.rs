use std::{env, fs, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    rpc_url: String,
    network_passphrase: String,
    vault_contract_id: String,
    oracle_contract_id: String,
    token_contract_id: String,
    start_ledger: Option<u64>,
    poll_interval_secs: u64,
    min_profit_threshold: i64,
    max_liquidations_per_run: usize,
    gas_budget: u32,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub vault_contract_id: String,
    pub oracle_contract_id: String,
    pub token_contract_id: String,
    pub start_ledger: Option<u64>,
    pub liquidator_secret_key: String,
    pub poll_interval_secs: u64,
    pub min_profit_threshold: i128,
    pub max_liquidations_per_run: usize,
    pub gas_budget: u32,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed reading config file {}", path.display()))?;
        let parsed: RawConfig =
            toml::from_str(&raw).context("failed parsing liquidation bot config")?;
        let liquidator_secret_key = env::var("LIQUIDATOR_SECRET_KEY")
            .context("LIQUIDATOR_SECRET_KEY must be set in the environment")?;

        Ok(Self {
            rpc_url: parsed.rpc_url,
            network_passphrase: parsed.network_passphrase,
            vault_contract_id: parsed.vault_contract_id,
            oracle_contract_id: parsed.oracle_contract_id,
            token_contract_id: parsed.token_contract_id,
            start_ledger: parsed.start_ledger,
            liquidator_secret_key,
            poll_interval_secs: parsed.poll_interval_secs,
            min_profit_threshold: i128::from(parsed.min_profit_threshold),
            max_liquidations_per_run: parsed.max_liquidations_per_run,
            gas_budget: parsed.gas_budget,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::Config;

    const TEST_SECRET: &str = "SBR4GQZDG3Q7UQWJ4OQ3NQ5G2LJQ3P2Q6P5KQ6LQ4S2D6M3R7H3L6KQ";

    #[test]
    fn loads_example_config_with_env_secret() {
        let path = write_temp_config(include_str!("../config.example.toml"));
        let previous = std::env::var("LIQUIDATOR_SECRET_KEY").ok();
        std::env::set_var("LIQUIDATOR_SECRET_KEY", TEST_SECRET);

        let config = Config::load(&path).expect("example config should parse");

        assert_eq!(config.rpc_url, "https://soroban-testnet.stellar.org");
        assert_eq!(
            config.network_passphrase,
            "Test SDF Network ; September 2015"
        );
        assert_eq!(config.liquidator_secret_key, TEST_SECRET);
        assert_eq!(config.start_ledger, None);
        assert!(config.poll_interval_secs > 0);
        assert!(config.max_liquidations_per_run > 0);

        restore_secret(previous);
        let _ = fs::remove_file(path);
    }

    fn write_temp_config(contents: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("helix-liquidation-bot-{suffix}.toml"));
        fs::write(&path, contents).expect("temporary config should be writable");
        path
    }

    fn restore_secret(previous: Option<String>) {
        if let Some(value) = previous {
            std::env::set_var("LIQUIDATOR_SECRET_KEY", value);
        } else {
            std::env::remove_var("LIQUIDATOR_SECRET_KEY");
        }
    }
}
