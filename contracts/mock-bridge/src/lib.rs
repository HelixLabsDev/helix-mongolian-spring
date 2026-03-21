#![no_std]

use soroban_sdk::{contract, contractimpl, Address, Bytes, Env};

/// Mock bridge contract for local/testnet testing.
/// Simulates Axelar GMP message receipt without actual cross-chain calls.
/// Used in integration tests for deposit/withdrawal flows.
#[contract]
pub struct MockBridge;

#[contractimpl]
impl MockBridge {
    /// Simulate receiving a cross-chain deposit message.
    /// In production, this comes from Axelar GMP execute().
    pub fn mock_deposit(
        env: Env,
        token_contract: Address,
        recipient: Address,
        amount: i128,
        _source_chain: Bytes,
    ) {
        let _ = (env, token_contract, recipient, amount);
        todo!("Phase 1/2: mint hstETH to recipient via token contract")
    }

    /// Simulate a yield update message from L1.
    pub fn mock_yield_update(env: Env, token_contract: Address, new_total_assets: i128) {
        let _ = (env, token_contract, new_total_assets);
        todo!("Phase 1: update exchange rate on token contract")
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_placeholder() {
        assert!(true);
    }
}
