import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  callRpc,
  readFreighterWallet,
  readInjectedPosition,
  readLiveDashboardInputs,
  readRpcStatus,
  resolveRpcUrl,
  shortAddress,
} from "./src/liveData.mjs";

describe("stellar dashboard live data", () => {
  it("resolves RPC URL from query before storage default", () => {
    assert.equal(
      resolveRpcUrl({
        location: { search: "?rpc=https%3A%2F%2Fexample.invalid%2Frpc" },
        storage: { getItem: () => "https://stored.invalid" },
      }),
      "https://example.invalid/rpc"
    );
  });

  it("calls Stellar RPC JSON-RPC endpoints", async () => {
    const result = await callRpc({
      rpcUrl: "https://rpc.invalid",
      method: "getHealth",
      fetchImpl: async (url, request) => {
        assert.equal(url, "https://rpc.invalid");
        assert.equal(JSON.parse(request.body).method, "getHealth");
        return {
          ok: true,
          async json() {
            return { result: { status: "healthy" } };
          },
        };
      },
    });

    assert.deepEqual(result, { status: "healthy" });
  });

  it("normalizes RPC health and latest ledger", async () => {
    const responses = [
      { status: "healthy" },
      { sequence: 1234, protocolVersion: 22 },
    ];
    const status = await readRpcStatus({
      rpcUrl: "https://rpc.invalid",
      fetchImpl: async () => ({
        ok: true,
        async json() {
          return { result: responses.shift() };
        },
      }),
    });

    assert.equal(status.live, true);
    assert.equal(status.status, "healthy");
    assert.equal(status.latestLedger.sequence, 1234);
  });

  it("reads Freighter wallet variants without leaking raw APIs into UI", async () => {
    const wallet = await readFreighterWallet({
      freighterApi: {
        requestAccess: async () => ({ publicKey: "GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890" }),
      },
    });

    assert.equal(wallet.status, "connected");
    assert.equal(wallet.label, "GABCDEFG...567890");
  });

  it("does not request Freighter access during passive live refresh", async () => {
    let requested = false;
    const inputs = await readLiveDashboardInputs({
      globalObject: {
        location: { search: "" },
        localStorage: { getItem: () => null },
        freighterApi: {
          requestAccess: async () => {
            requested = true;
            return "GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";
          },
        },
      },
      fetchImpl: async () => ({
        ok: true,
        async json() {
          return { result: { status: "healthy" } };
        },
      }),
    });

    assert.equal(requested, false);
    assert.equal(inputs.wallet.status, "available");
  });

  it("reads injected contract facades through the shared position reader", async () => {
    const result = await readInjectedPosition({
      globalObject: {
        freighterApi: { getPublicKey: async () => "GUSERSTELLARADDRESS" },
        helixStellarContracts: createContractFacades(),
      },
    });

    assert.equal(result.status, "live");
    assert.equal(result.position.chain, "stellar");
    assert.equal(result.position.collateral_token, "hstETH");
  });

  it("shortens addresses", () => {
    assert.equal(shortAddress("GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890"), "GABCDEFG...567890");
  });
});

function createContractFacades() {
  return {
    vault: {
      contractId: "CVAULTCONTRACT",
      async getPositionSnapshot() {
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
      async assetsForShares() {
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
