use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::{
    config::Config,
    rpc::{parse_address, parse_event_name, parse_vec, RpcClient},
    types::{PositionKey, TrackedPosition},
};

const INITIAL_LEDGER_LOOKBACK: u64 = 1_000;
const EVENT_PAGE_LIMIT: u32 = 200;
const TRACKED_EVENTS: &[&str] = &["deposit", "borrow", "repay", "withdraw", "liquidation"];
const PRIMARY_ASSET_PLACEHOLDER: &str = "__primary_asset__";

#[derive(Debug, Default)]
pub struct ScannerState {
    positions: HashMap<PositionKey, TrackedPosition>,
    seen_event_ids: HashSet<String>,
    user_assets: HashMap<String, String>,
    last_seen_ledger: Option<u64>,
    primary_asset: Option<String>,
}

impl ScannerState {
    pub fn tracked_positions(&self) -> impl Iterator<Item = &TrackedPosition> {
        self.positions.values()
    }

    pub fn remove_user(&mut self, user: &str) {
        self.positions.retain(|key, _| key.user != user);
        self.user_assets.remove(user);
    }
}

pub async fn scan(config: &Config, rpc: &RpcClient, state: &mut ScannerState) -> Result<usize> {
    if state.primary_asset.is_none() {
        state.primary_asset = rpc
            .read_supported_asset(&config.vault_contract_id)
            .await
            .context("failed reading supported asset from vault instance storage")?;
    }

    let latest_ledger = rpc.latest_ledger().await?;
    let from_ledger = state
        .last_seen_ledger
        .map(|ledger| ledger.saturating_add(1))
        .unwrap_or_else(|| latest_ledger.saturating_sub(INITIAL_LEDGER_LOOKBACK));

    debug!(from_ledger, latest_ledger, "scanning vault events");
    let response = rpc
        .get_events(
            from_ledger,
            &config.vault_contract_id,
            TRACKED_EVENTS,
            EVENT_PAGE_LIMIT,
        )
        .await?;

    let mut updates = 0usize;

    for event in response.events {
        if !state.seen_event_ids.insert(event.id.clone()) {
            continue;
        }

        let topic = event.topic();
        if topic.is_empty() {
            continue;
        }

        let event_name = parse_event_name(&topic[0])
            .with_context(|| format!("failed parsing event name for event {}", event.id))?;

        let user = match event_name.as_str() {
            "deposit" | "borrow" | "repay" | "withdraw" => topic.get(1).map(parse_address),
            "liquidation" => topic.get(2).map(parse_address),
            _ => None,
        }
        .transpose()?;

        let Some(user) = user else {
            warn!(event_id = %event.id, event_name, "skipping event without user topic");
            continue;
        };

        let asset = if event_name == "deposit" {
            let values = parse_vec(&event.value())
                .with_context(|| format!("failed parsing deposit event body for {}", event.id))?;
            let deposit_asset = parse_address(
                values
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("deposit event missing asset"))?,
            )?;
            state
                .user_assets
                .insert(user.clone(), deposit_asset.clone());
            deposit_asset
        } else {
            state
                .user_assets
                .get(&user)
                .cloned()
                .or_else(|| state.primary_asset.clone())
                .unwrap_or_else(|| PRIMARY_ASSET_PLACEHOLDER.to_string())
        };

        match rpc.get_position(&config.vault_contract_id, &user).await? {
            Some(position) => {
                let key = PositionKey {
                    user: user.clone(),
                    asset,
                };
                let tracked = TrackedPosition {
                    key: key.clone(),
                    position,
                    last_event_ledger: event.ledger,
                };
                state.positions.insert(key, tracked);
                updates += 1;
            }
            None => {
                state.remove_user(&user);
                info!(user, "removed fully closed position from scanner index");
            }
        }

        state.last_seen_ledger = Some(event.ledger);
    }

    if state.last_seen_ledger.is_none() {
        state.last_seen_ledger = Some(latest_ledger);
    }

    Ok(updates)
}
