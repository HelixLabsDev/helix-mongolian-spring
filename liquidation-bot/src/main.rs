mod config;
mod executor;
mod health;
mod rpc;
mod scanner;
mod types;

use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use clap::Parser;
use config::Config;
use rpc::RpcClient;
use scanner::ScannerState;
use tokio::time::sleep;
use tracing::{info, warn};
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Parser)]
#[command(author, version, about = "Helix Stellar liquidation bot")]
struct Cli {
    #[arg(long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)?;
    let rpc = RpcClient::new(&config)?;
    let mut scanner_state = ScannerState::default();

    info!(
        rpc_url = %config.rpc_url,
        vault_contract_id = %config.vault_contract_id,
        oracle_contract_id = %config.oracle_contract_id,
        token_contract_id = %config.token_contract_id,
        "starting liquidation bot"
    );

    loop {
        tokio::select! {
            result = run_iteration(&config, &rpc, &mut scanner_state) => {
                if let Err(error) = result {
                    warn!(error = %error, "liquidation iteration failed");
                }
            }
            _ = shutdown_signal() => {
                info!("received shutdown signal, exiting");
                break;
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(config.poll_interval_secs)) => {}
            _ = shutdown_signal() => {
                info!("received shutdown signal during sleep, exiting");
                break;
            }
        }
    }

    Ok(())
}

async fn run_iteration(
    config: &Config,
    rpc: &RpcClient,
    scanner_state: &mut ScannerState,
) -> Result<()> {
    let updates = scanner::scan(config, rpc, scanner_state).await?;
    info!(updates, "scanner pass completed");

    let candidates = health::evaluate(config, rpc, scanner_state).await?;
    info!(
        candidate_count = candidates.len(),
        "health evaluation completed"
    );

    let submitted = executor::execute(config, rpc, &candidates).await?;
    info!(submitted, "executor pass completed");

    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut interrupt =
            signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
        let mut terminate =
            signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,helix_liquidation_bot=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .compact()
        .init();
}
