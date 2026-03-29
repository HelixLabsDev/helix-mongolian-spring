use anyhow::Result;
use tracing::{debug, warn};

use crate::{
    config::Config,
    rpc::RpcClient,
    scanner::ScannerState,
    types::{
        HealthAssessment, PoolConfig, TrackedPosition, DEFAULT_LIQUIDATION_BONUS_BPS,
        HEALTH_FACTOR_ONE,
    },
};

pub async fn evaluate(
    config: &Config,
    rpc: &RpcClient,
    state: &ScannerState,
) -> Result<Vec<HealthAssessment>> {
    let pool_config = rpc.read_pool_config(&config.vault_contract_id).await?;
    let liquidation_bonus_bps = pool_config
        .as_ref()
        .map(|cfg| cfg.liq_bonus)
        .unwrap_or(DEFAULT_LIQUIDATION_BONUS_BPS);

    let mut results = Vec::new();

    for tracked in state.tracked_positions() {
        let health_factor = rpc
            .get_health_factor(&config.vault_contract_id, &tracked.key.user)
            .await?;

        if health_factor >= HEALTH_FACTOR_ONE {
            debug!(
                user = %tracked.key.user,
                asset = %tracked.key.asset,
                health_factor,
                "position is healthy"
            );
            continue;
        }

        let (oracle_price, oracle_timestamp) = if tracked.key.asset == "__primary_asset__" {
            warn!(
                user = %tracked.key.user,
                "asset unknown from scanner, skipping oracle price lookup"
            );
            (0, 0)
        } else {
            rpc.last_price(&config.oracle_contract_id, &tracked.key.asset)
                .await?
        };

        let repay_amount = liquidation_repay_amount(tracked);
        let expected_profit = expected_profit(repay_amount, liquidation_bonus_bps);

        results.push(HealthAssessment {
            tracked: tracked.clone(),
            health_factor,
            oracle_price,
            oracle_timestamp,
            expected_profit,
        });
    }

    results.sort_by_key(|assessment| assessment.health_factor);
    Ok(results)
}

fn liquidation_repay_amount(tracked: &TrackedPosition) -> i128 {
    tracked.position.borrowed_amount / 2
}

fn expected_profit(repay_amount: i128, liq_bonus_bps: u32) -> i128 {
    repay_amount
        .saturating_mul(i128::from(liq_bonus_bps))
        .saturating_div(10_000)
}

#[allow(dead_code)]
fn _pool_config_bonus(pool_config: Option<&PoolConfig>) -> u32 {
    pool_config
        .map(|cfg| cfg.liq_bonus)
        .unwrap_or(DEFAULT_LIQUIDATION_BONUS_BPS)
}
