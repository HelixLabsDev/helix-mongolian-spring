#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

/// Storage keys for the Helix collateral vault.
#[contracttype]
pub enum DataKey {
    Admin,
    OracleAddress,
    Paused,
    Locked,
    SupportedAssets,
    Position(Address, Address), // (user, asset) → position data
    Role(Address, u32),         // (address, role_id) → bool
}

/// A user's collateral position for a specific asset.
#[contracttype]
#[derive(Clone, Debug)]
pub struct Position {
    pub deposited_shares: i128,
    pub borrowed_amount: i128,
    pub last_update: u64,
}

#[contract]
pub struct HelixVault;

#[contractimpl]
impl HelixVault {
    pub fn initialize(env: Env, admin: Address, oracle: Address) {
        let _ = (env, admin, oracle);
        todo!("Phase 2: implement initialization")
    }

    pub fn deposit(env: Env, user: Address, token: Address, amount: i128) {
        let _ = (env, user, token, amount);
        todo!("Phase 2: implement deposit")
    }

    pub fn withdraw(env: Env, user: Address, token: Address, amount: i128) {
        let _ = (env, user, token, amount);
        todo!("Phase 2: implement withdraw")
    }

    pub fn get_health_factor(env: Env, user: Address) -> i128 {
        let _ = (env, user);
        todo!("Phase 2: implement health factor")
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_placeholder() {
        assert!(true);
    }
}
