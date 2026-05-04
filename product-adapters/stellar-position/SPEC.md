# Stellar Position Adapter Spec

## Purpose

This adapter is the read-only L4 boundary between Stellar contract reads and Terminal. Terminal consumes `HelixPosition`; it must not parse Soroban `ScVal`, vault storage keys, or liquidation-bot internals.

`reader.mjs` fills `StellarVaultSnapshot` from injected Freighter/RPC contract facades. `adapter.mjs` maps that snapshot into the shared Terminal DTO. SDK transport setup, raw `ScVal` parsing, and UI wiring stay outside this package.

## Output Contract

`HelixPosition` follows `helix-position.schema.json` and the Stellar architecture plan:

```ts
type HelixPosition = {
  chain: "stellar";
  user_id: string;
  position_id: string;
  collateral_token: string;
  collateral_amount: number;
  collateral_value_usd: number;
  debt_token: string;
  debt_amount: number;
  debt_value_usd: number;
  health_factor: number;
  ltv_current: number;
  ltv_max: number;
  liquidation_threshold: number;
  borrow_rate: number;
  accrued_interest: number;
  last_updated: string;
  status: "active" | "liquidatable" | "liquidated" | "closed";
};
```

`last_updated` is an ISO-8601 timestamp. Ratios are decimal ratios, not bps: `0.75` means 75%.

## Input Snapshot

`StellarVaultSnapshot` is intentionally one layer above raw Soroban values:

```ts
type StellarVaultSnapshot = {
  userAddress: string;
  vaultContractId: string;
  position: {
    deposited_shares: string | number | bigint;
    borrowed_amount: string | number | bigint;
    last_update: string | number | bigint;
  };
  poolConfig: {
    max_ltv: string | number | bigint;
    liq_threshold: string | number | bigint;
    interest_rate: string | number | bigint;
  };
  collateral: {
    tokenAddress: string;
    symbol: string;
    decimals: string | number | bigint;
    assets: string | number | bigint;
  };
  debt: {
    tokenAddress: string;
    symbol: string;
    decimals: string | number | bigint;
  };
  prices: {
    collateralUsd: number;
    debtUsd: number;
  };
  healthFactorBps: string | number | bigint;
  accruedInterest: string | number | bigint;
  asOf?: string | number | bigint;
  status?: "active" | "liquidatable" | "liquidated" | "closed";
};
```

## Reader Interface

`createStellarPositionReader(config)` returns a read-only reader:

```ts
type StellarPositionReaderConfig = {
  wallet: {
    getPublicKey?: () => Promise<string>;
    getAddress?: () => Promise<string>;
    requestAccess?: () => Promise<string | { address?: string; publicKey?: string }>;
  };
  contracts: {
    vault: {
      contractId: string;
      getPositionSnapshot: (user: string) => Promise<PositionSnapshot>;
    };
    token: {
      assetsForShares: (tokenAddress: string, shares: string) => Promise<string | number | bigint>;
      symbol: (tokenAddress: string) => Promise<string>;
      decimals: (tokenAddress: string) => Promise<string | number | bigint>;
    };
    oracle: {
      lastprice: (tokenAddress: string) => Promise<[string | number | bigint, unknown] | { price: string | number | bigint }>;
      decimals: (tokenAddress: string) => Promise<string | number | bigint>;
    };
  };
  debtPriceUsd?: number;
};
```

The reader accepts snake_case alternatives for Soroban-style facades: `contract_id`, `get_position_snapshot`, and `assets_for_shares`.

## Reader Responsibilities

The Stellar reader populates the snapshot from:

- Freighter: current Stellar public key.
- Vault `get_position_snapshot(user)`: `accrued_position`, `accrued_interest`, `health_factor`, `pool_config`, `collateral_token`, and `borrow_token`.
- hst token `assets_for_shares(deposited_shares)`: `collateral.assets`.
- Oracle `lastprice(collateralToken)` and known oracle decimals: `prices.collateralUsd`.
- Debt token metadata or stablecoin convention: `debt.symbol`, `debt.decimals`, `prices.debtUsd`.
- Vault `get_pool_config()`: optional direct pool-config read when a reader does not need the full snapshot.

For `position`, use `get_position_snapshot(user).accrued_position` so Terminal displays current accrued debt. For `accruedInterest`, use `get_position_snapshot(user).accrued_interest`; this is interest accrued since the last state-writing vault update, not lifetime interest since loan origination.

## Current Limitations

- This package does not instantiate Freighter or Stellar SDK clients.
- This package does not parse raw `ScVal`; injected contract facades must return normalized JavaScript values.
- This adapter does not infer lifetime principal/accrued-interest split from current vault state.
- This adapter does not make product decisions about cross-chain identity.
- This package does not wire Terminal UI states or persistence.

## Verification

Run:

```sh
node --test product-adapters/stellar-position/adapter.test.mjs product-adapters/stellar-position/reader.test.mjs
```
