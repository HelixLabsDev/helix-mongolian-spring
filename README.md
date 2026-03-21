# Helix Stellar — Soroban Core Infrastructure

Cross-chain LST collateral lending on Stellar. SCF #41 Build Award.

## Contracts

| Contract | Purpose | Phase |
|----------|---------|-------|
| `helix-token` | hstETH — SEP-41 non-rebasing vault receipt token | T1 Phase 1 |
| `helix-vault` | Collateral vault — deposit, borrow, liquidate | T1 Phase 2 |
| `helix-oracle-adaptor` | SEP-40 oracle with TWAP + multi-source fallback | T1 Phase 3 |
| `helix-mock-bridge` | Mock Axelar GMP for testing | T1 Phase 1-2 |

## Quick Start

```bash
# Setup toolchain + testnet identity
chmod +x setup.sh && ./setup.sh

# Build
make build

# Test
make test

# Build + optimize WASM
make optimize

# Check sizes
make sizes
```

## Architecture

```
User locks wstETH on Ethereum
    → Axelar GMP message
    → Stellar: mint hstETH (share-based, non-rebasing)
    → Deposit hstETH as collateral in Helix vault
    → Borrow against collateral at oracle-derived health factor
```

## License

TBD
