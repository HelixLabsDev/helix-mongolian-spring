import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  bpsToRatio,
  mapStellarVaultSnapshotToHelixPosition,
  scaleIntegerToNumber,
} from "./adapter.mjs";

const baseSnapshot = {
  userAddress: "GUSERSTELLARADDRESS",
  vaultContractId: "CVAULTCONTRACT",
  position: {
    deposited_shares: "400000000",
    borrowed_amount: "120000000",
    last_update: 1_700_000_000,
  },
  poolConfig: {
    max_ltv: 7_500,
    liq_threshold: 8_000,
    interest_rate: 300,
  },
  collateral: {
    tokenAddress: "CHSTETH",
    symbol: "hstETH",
    decimals: 7,
    assets: "400000000",
  },
  debt: {
    tokenAddress: "CUSDC",
    symbol: "USDC",
    decimals: 7,
  },
  prices: {
    collateralUsd: 2_500,
    debtUsd: 1,
  },
  healthFactorBps: 66_666,
  accruedInterest: "20000000",
};

describe("stellar position adapter", () => {
  it("maps a vault snapshot into the Terminal HelixPosition shape", () => {
    const position = mapStellarVaultSnapshotToHelixPosition(baseSnapshot);

    assert.deepEqual(position, {
      chain: "stellar",
      user_id: "GUSERSTELLARADDRESS",
      position_id: "stellar:CVAULTCONTRACT:GUSERSTELLARADDRESS",
      collateral_token: "hstETH",
      collateral_amount: 40,
      collateral_value_usd: 100_000,
      debt_token: "USDC",
      debt_amount: 12,
      debt_value_usd: 12,
      health_factor: 6.6666,
      ltv_current: 0.00012,
      ltv_max: 0.75,
      liquidation_threshold: 0.8,
      borrow_rate: 0.03,
      accrued_interest: 2,
      last_updated: "2023-11-14T22:13:20.000Z",
      status: "active",
    });
  });

  it("marks undercollateralized debt as liquidatable", () => {
    const position = mapStellarVaultSnapshotToHelixPosition({
      ...baseSnapshot,
      healthFactorBps: 9_999,
    });

    assert.equal(position.status, "liquidatable");
  });

  it("allows explicit terminal status when supplied by a future reader", () => {
    const position = mapStellarVaultSnapshotToHelixPosition({
      ...baseSnapshot,
      status: "liquidated",
    });

    assert.equal(position.status, "liquidated");
  });

  it("rejects malformed snapshots before Terminal sees partial DTOs", () => {
    assert.throws(
      () => mapStellarVaultSnapshotToHelixPosition({ ...baseSnapshot, healthFactorBps: "bad" }),
      /healthFactorBps/
    );
  });

  it("normalizes Soroban integer amounts and bps values", () => {
    assert.equal(scaleIntegerToNumber("123456789", 7), 12.3456789);
    assert.equal(scaleIntegerToNumber("-120000000", 7), -12);
    assert.equal(bpsToRatio(7_500), 0.75);
  });
});
