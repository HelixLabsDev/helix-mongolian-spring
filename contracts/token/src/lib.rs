#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

/// Storage keys for the hstETH token contract.
/// Non-rebasing share-based design: constant shares, increasing exchange_rate.
#[contracttype]
pub enum DataKey {
    Admin,
    Name,
    Symbol,
    Decimals,
    TotalShares,
    TotalAssets,
    Balance(Address),
    Allowance(Address, Address),
}

#[contract]
pub struct HelixToken;

#[contractimpl]
impl HelixToken {
    /// Initialize the hstETH token.
    /// Called once by deployer. Sets admin, name, symbol, decimals.
    pub fn initialize(
        env: Env,
        admin: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) {
        // Phase 1: Full implementation
        // - Store admin, name, symbol, decimals
        // - Initialize TotalShares = 0, TotalAssets = 0
        // - Set TTL extensions
        let _ = (env, admin, name, symbol, decimals);
        todo!("Phase 1: implement initialization")
    }

    /// Current exchange rate: TotalAssets / TotalShares.
    /// Returns in fixed-point (decimals match token decimals).
    pub fn exchange_rate(env: Env) -> i128 {
        let _ = env;
        todo!("Phase 1: implement exchange_rate")
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_placeholder() {
        let _env = soroban_sdk::Env::default();
        assert!(true);
    }
}
