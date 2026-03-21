#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

/// SEP-40 compliant oracle adaptor.
/// Wraps Reflector (primary) + DIA (secondary) with TWAP and deviation checks.
#[contracttype]
pub enum DataKey {
    Admin,
    PrimaryOracle,
    SecondaryOracle,
    TWAPWindow,
    StalenessThreshold,
    DeviationThreshold,
}

#[contract]
pub struct HelixOracleAdaptor;

#[contractimpl]
impl HelixOracleAdaptor {
    /// SEP-40: Returns the last price for an asset.
    pub fn lastprice(env: Env, asset: Address) -> Option<i128> {
        let _ = (env, asset);
        todo!("Phase 3: implement SEP-40 lastprice")
    }

    /// SEP-40: Returns the number of decimals for the price.
    pub fn decimals(env: Env, asset: Address) -> u32 {
        let _ = (env, asset);
        todo!("Phase 3: implement SEP-40 decimals")
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_placeholder() {
        assert!(true);
    }
}
