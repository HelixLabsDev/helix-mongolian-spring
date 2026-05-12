import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  createHelixStellarContracts,
  HELIX_STELLAR_CONTRACT_IDS,
  installHelixStellarContracts,
} from "./src/contractFacades.mjs";
import {
  callRpc,
  FREIGHTER_API_SCRIPT_URL,
  loadFreighterApi,
  readFreighterWallet,
  readInjectedPosition,
  readLiveDashboardInputs,
  readRpcStatus,
  resolveRpcUrl,
  shortAddress,
} from "./src/liveData.mjs";
import { dashboardSnapshot } from "./src/dashboardModel.mjs";

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

  it("loads the Freighter browser API bridge when the extension API is not injected", async () => {
    const appendedScripts = [];
    const globalObject = {
      document: {
        createElement(tagName) {
          assert.equal(tagName, "script");
          return {};
        },
        head: {
          appendChild(script) {
            appendedScripts.push(script);
            globalObject.freighterApi = {
              requestAccess: async () => "GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890",
            };
            script.onload();
          },
        },
      },
    };

    const api = await loadFreighterApi(globalObject);

    assert.equal(api, globalObject.freighterApi);
    assert.equal(appendedScripts.length, 1);
    assert.equal(appendedScripts[0].src, FREIGHTER_API_SCRIPT_URL);
    assert.equal(await loadFreighterApi(globalObject), api);
    assert.equal(appendedScripts.length, 1);
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

  it("classifies missing wallet vault positions separately from generic read failures", async () => {
    const contracts = createContractFacades();
    contracts.vault.getPositionSnapshot = async () => {
      throw new Error("position not found");
    };

    const result = await readInjectedPosition({
      globalObject: {
        helixStellarContracts: contracts,
      },
      wallet: { getPublicKey: async () => "GEMPTYWALLET" },
      useGlobalWalletFallback: false,
    });

    assert.equal(result.status, "not_found");
    assert.equal(result.position, null);
  });

  it("shortens addresses", () => {
    assert.equal(shortAddress("GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890"), "GABCDEFG...567890");
  });

  it("creates SDK-backed contract facades for the shared position reader", async () => {
    const contracts = await createHelixStellarContracts({
      stellarSdk: createFakeStellarSdk(),
    });

    const result = await readInjectedPosition({
      globalObject: {
        freighterApi: { getPublicKey: async () => "GUSERSTELLARADDRESS" },
        helixStellarContracts: contracts,
      },
    });

    assert.equal(result.status, "live");
    assert.equal(result.position.collateral_token, "hstETH");
    assert.equal(result.position.debt_token, "XLM");
  });

  it("installs contract facades without replacing an existing injection", async () => {
    const existing = { sentinel: true };
    const globalObject = { helixStellarContracts: existing };

    assert.equal(await installHelixStellarContracts(globalObject, { stellarSdk: createFakeStellarSdk() }), existing);

    const replaced = await installHelixStellarContracts(globalObject, {
      stellarSdk: createFakeStellarSdk(),
      force: true,
    });
    assert.notEqual(replaced, existing);
    assert.equal(globalObject.helixStellarContracts, replaced);
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

function createFakeStellarSdk() {
  class Spec {
    constructor(entries) {
      this.entries = entries;
    }
  }

  class Client {
    constructor(_spec, options) {
      this.options = options;
    }

    async get_position_snapshot({ user }) {
      assert.equal(user, "GUSERSTELLARADDRESS");
      return transactionResult({
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
        collateral_token: HELIX_STELLAR_CONTRACT_IDS.token,
        borrow_token: dashboardSnapshot.debt.tokenAddress,
      });
    }

    async assets_for_shares({ shares }) {
      assert.equal(this.options.contractId, HELIX_STELLAR_CONTRACT_IDS.token);
      assert.equal(shares, "400000000");
      return transactionResult("400000000");
    }

    async symbol() {
      return transactionResult("hstETH");
    }

    async decimals() {
      return transactionResult("7");
    }

    async lastprice({ asset }) {
      assert.equal(asset, HELIX_STELLAR_CONTRACT_IDS.token);
      return transactionResult(["25000000000", "1700000000"]);
    }
  }

  return {
    Networks: {
      TESTNET: "Test SDF Network ; September 2015",
    },
    contract: {
      Client,
      Spec,
    },
  };
}

function transactionResult(result) {
  return {
    get result() {
      return result;
    },
    toJSON() {
      return {
        method: "fake",
        simulationResult: {
          retval: result,
        },
      };
    },
  };
}
