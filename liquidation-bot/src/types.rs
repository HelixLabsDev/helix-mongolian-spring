use std::hash::{Hash, Hasher};

pub const HEALTH_FACTOR_ONE: i128 = 10_000;
pub const DEFAULT_LIQUIDATION_BONUS_BPS: u32 = 500;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Position {
    pub deposited_shares: i128,
    pub borrowed_amount: i128,
    pub last_update: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PoolConfig {
    pub max_ltv: u32,
    pub liq_threshold: u32,
    pub liq_bonus: u32,
    pub interest_rate: u32,
    pub min_position: i128,
}

#[derive(Debug, Clone, Eq)]
pub struct PositionKey {
    pub user: String,
    pub asset: String,
}

impl PartialEq for PositionKey {
    fn eq(&self, other: &Self) -> bool {
        self.user == other.user && self.asset == other.asset
    }
}

impl Hash for PositionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.user.hash(state);
        self.asset.hash(state);
    }
}

#[derive(Debug, Clone)]
pub struct TrackedPosition {
    pub key: PositionKey,
    pub position: Position,
    pub last_event_ledger: u64,
}

#[derive(Debug, Clone)]
pub struct HealthAssessment {
    pub tracked: TrackedPosition,
    pub health_factor: i128,
    pub oracle_price: i128,
    pub oracle_timestamp: u64,
    pub expected_profit: i128,
}
