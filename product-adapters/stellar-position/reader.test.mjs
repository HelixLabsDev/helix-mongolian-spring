import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  createStellarPositionReader,
  normalizePositionSnapshot,
  readStellarVaultSnapshot,
  readWalletAddress,
} from "./reader.mjs";

const userAddress = "GUSERSTELLARADDRESS";

function createContractFacades(calls = []) {
  return {
    vault: {
      contract_id: "CVAULTCONTRACT",
      async get_position_snapshot(user) {
        calls.push(["snapshot", user]);
        return {
          stored_position: {
            deposited_shares: "400000000",
            borrowed_amount: "100000000",
            last_update: "1699990000",
          },
          accrued_position: {
            deposited_shares: "400000000",
            borrowed_amount: "120000000",
            last_update: "1700000000",
          },
          accrued_interest: "20000000",
          health_factor: "66666",
          pool_config: {
            max_ltv: "7500",
            liq_threshold: "8000",
            interest_rate: "300",
          },
          collateral_token: "CHSTETH",
          borrow_token: "CUSDC",
        };
      },
    },
    token: {
      async assets_for_shares(tokenAddress, shares) {
        calls.push(["assets", tokenAddress, shares]);
        return "400000000";
      },
      async symbol(tokenAddress) {
        return tokenAddress === "CHSTETH" ? "hstETH" : "USDC";
      },
      async decimals() {
        return "7";
      },
    },
    oracle: {
      async lastprice() {
        return ["25000000000", "1700000000"];
      },
      async decimals() {
        return 7;
      },
    },
  };
}

describe("stellar position reader", () => {
  it("reads a StellarVaultSnapshot from wallet and contract facades", async () => {
    const calls = [];
    const contracts = createContractFacades(calls);

    const snapshot = await readStellarVaultSnapshot({
      wallet: { getPublicKey: async () => userAddress },
      ...contracts,
    });

    assert.equal(snapshot.userAddress, userAddress);
    assert.equal(snapshot.vaultContractId, "CVAULTCONTRACT");
    assert.deepEqual(snapshot.position, {
      deposited_shares: "400000000",
      borrowed_amount: "120000000",
      last_update: "1700000000",
    });
    assert.deepEqual(snapshot.poolConfig, {
      max_ltv: "7500",
      liq_threshold: "8000",
      interest_rate: "300",
    });
    assert.deepEqual(snapshot.collateral, {
      tokenAddress: "CHSTETH",
      symbol: "hstETH",
      decimals: "7",
      assets: "400000000",
    });
    assert.deepEqual(snapshot.debt, {
      tokenAddress: "CUSDC",
      symbol: "USDC",
      decimals: "7",
    });
    assert.deepEqual(snapshot.prices, {
      collateralUsd: 2500,
      debtUsd: 1,
    });
    assert.equal(snapshot.healthFactorBps, "66666");
    assert.equal(snapshot.accruedInterest, "20000000");
    assert.equal(snapshot.asOf, "1700000000");
    assert.deepEqual(calls, [
      ["snapshot", userAddress],
      ["assets", "CHSTETH", "400000000"],
    ]);
  });

  it("maps the read snapshot into the Terminal HelixPosition shape", async () => {
    const reader = createStellarPositionReader({
      wallet: { getAddress: async () => userAddress },
      contracts: createContractFacades(),
    });

    const position = await reader.readPosition();

    assert.equal(position.chain, "stellar");
    assert.equal(position.user_id, userAddress);
    assert.equal(position.position_id, `stellar:CVAULTCONTRACT:${userAddress}`);
    assert.equal(position.collateral_amount, 40);
    assert.equal(position.collateral_value_usd, 100000);
    assert.equal(position.debt_amount, 12);
    assert.equal(position.debt_value_usd, 12);
    assert.equal(position.health_factor, 6.6666);
    assert.equal(position.ltv_max, 0.75);
    assert.equal(position.liquidation_threshold, 0.8);
    assert.equal(position.borrow_rate, 0.03);
    assert.equal(position.accrued_interest, 2);
    assert.equal(position.last_updated, "2023-11-14T22:13:20.000Z");
    assert.equal(position.status, "active");
  });

  it("supports Freighter requestAccess and address-like objects", async () => {
    assert.equal(
      await readWalletAddress({ requestAccess: async () => ({ publicKey: userAddress }) }),
      userAddress
    );

    const address = { toString: () => userAddress };
    assert.equal(await readWalletAddress({ getAddress: async () => address }), userAddress);
  });

  it("normalizes camelCase snapshot fields from generated clients", () => {
    assert.deepEqual(
      normalizePositionSnapshot({
        storedPosition: {
          depositedShares: 1n,
          borrowedAmount: 2n,
          lastUpdate: 3n,
        },
        accruedPosition: {
          depositedShares: 4,
          borrowedAmount: 5,
          lastUpdate: 6,
        },
        accruedInterest: "7",
        healthFactor: "8",
        poolConfig: {
          maxLtv: 7500,
          liqThreshold: 8000,
          interestRate: 300,
        },
        collateralToken: "CHSTETH",
        borrowToken: "CUSDC",
      }),
      {
        stored_position: {
          deposited_shares: "1",
          borrowed_amount: "2",
          last_update: "3",
        },
        accrued_position: {
          deposited_shares: "4",
          borrowed_amount: "5",
          last_update: "6",
        },
        accrued_interest: "7",
        health_factor: "8",
        pool_config: {
          max_ltv: "7500",
          liq_threshold: "8000",
          interest_rate: "300",
        },
        collateral_token: "CHSTETH",
        borrow_token: "CUSDC",
      }
    );
  });

  it("rejects incomplete wallet and snapshot inputs", async () => {
    await assert.rejects(readWalletAddress({}), /wallet must expose/);

    assert.throws(
      () => normalizePositionSnapshot({ accrued_position: {}, pool_config: {} }),
      /stored_position or storedPosition/
    );
  });
});
