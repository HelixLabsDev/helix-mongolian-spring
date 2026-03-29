use anyhow::Result;
use tracing::{info, warn};

use crate::{config::Config, rpc::RpcClient, types::HealthAssessment};

pub async fn execute(
    config: &Config,
    rpc: &RpcClient,
    candidates: &[HealthAssessment],
) -> Result<usize> {
    let liquidator = rpc.liquidator_public_key();
    let mut submitted = 0usize;

    for candidate in candidates {
        if submitted >= config.max_liquidations_per_run {
            break;
        }

        let repay_amount = candidate.tracked.position.borrowed_amount / 2;
        if repay_amount <= 0 {
            continue;
        }

        if candidate.expected_profit < config.min_profit_threshold {
            info!(
                user = %candidate.tracked.key.user,
                asset = %candidate.tracked.key.asset,
                expected_profit = candidate.expected_profit,
                min_profit_threshold = config.min_profit_threshold,
                oracle_price = candidate.oracle_price,
                oracle_timestamp = candidate.oracle_timestamp,
                last_event_ledger = candidate.tracked.last_event_ledger,
                "skipping liquidation below profitability threshold"
            );
            continue;
        }

        let liquidator_balance = rpc
            .token_balance(&config.token_contract_id, &liquidator)
            .await?;
        if liquidator_balance < repay_amount {
            warn!(
                user = %candidate.tracked.key.user,
                asset = %candidate.tracked.key.asset,
                liquidator_balance,
                repay_amount,
                oracle_price = candidate.oracle_price,
                oracle_timestamp = candidate.oracle_timestamp,
                last_event_ledger = candidate.tracked.last_event_ledger,
                "skipping liquidation due to insufficient liquidator funds"
            );
            continue;
        }

        match rpc
            .liquidate(config, &candidate.tracked.key.user, repay_amount)
            .await
        {
            Ok(result) => {
                submitted += 1;
                info!(
                    user = %candidate.tracked.key.user,
                    asset = %candidate.tracked.key.asset,
                    tx_hash = %result.hash,
                    tx_status = %result.status,
                    simulated_resource_fee = result.simulated_resource_fee,
                    hash_surrogate = %result.hash_surrogate,
                    oracle_price = candidate.oracle_price,
                    oracle_timestamp = candidate.oracle_timestamp,
                    last_event_ledger = candidate.tracked.last_event_ledger,
                    "submitted liquidation transaction"
                );
            }
            Err(error) => {
                warn!(
                    user = %candidate.tracked.key.user,
                    asset = %candidate.tracked.key.asset,
                    oracle_price = candidate.oracle_price,
                    oracle_timestamp = candidate.oracle_timestamp,
                    last_event_ledger = candidate.tracked.last_event_ledger,
                    error = %error,
                    "simulation or submission failed, liquidation skipped"
                );
            }
        }
    }

    Ok(submitted)
}
