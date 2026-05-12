import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  buildDashboardModel,
  dashboardSnapshot,
  formatCurrency,
  formatPercent,
} from "./src/dashboardModel.mjs";

describe("stellar dashboard model", () => {
  it("builds dashboard state from the shared Stellar position adapter", () => {
    const model = buildDashboardModel(dashboardSnapshot);

    assert.equal(model.position.chain, "stellar");
    assert.equal(model.position.collateral_token, "hstETH");
    assert.equal(model.position.status, "active");
    assert.equal(model.bridgeProof.status, "executed");
    assert.equal(model.bridgeProof.displayAmount, "0.01 WETH");
    assert.equal(model.ticker[0].value, "0.01 WETH");
    assert.equal(model.blend.unitTests, "10/10");
    assert.equal(
      model.readiness.find((item) => item.label === "T3 Evidence Packet").state,
      "complete"
    );
    assert.equal(
      model.readiness.find((item) => item.label === "Freighter Dashboard").state,
      "needs live verification"
    );
  });

  it("formats institutional dashboard values consistently", () => {
    assert.equal(formatCurrency(100000), "$100,000");
    assert.equal(formatPercent(0.00012), "0.01%");
  });

  it("makes connected-wallet fallback explicit when live position is unavailable", () => {
    const model = buildDashboardModel(dashboardSnapshot, {
      wallet: {
        address: "GCONNECTEDWALLET",
        status: "connected",
        label: "GCONNEC...WALLET",
      },
      injectedPosition: {
        status: "unavailable",
        error: "contract read failed",
      },
    });

    assert.equal(model.wallet.address, "GCONNECTEDWALLET");
    assert.equal(model.positionMode, "wallet fallback");
    assert.equal(model.positionSource.detail, "live wallet read unavailable");
    assert.equal(
      model.readiness.find((item) => item.label === "Position Adapter").state,
      "fallback"
    );
  });

  it("does not display a seeded wallet address before Freighter connects", () => {
    const model = buildDashboardModel(dashboardSnapshot, {
      wallet: {
        address: null,
        status: "available",
        label: "Connect Freighter",
      },
    });

    assert.equal(model.wallet.address, null);
    assert.equal(model.positionMode, "static evidence");
  });

  it("distinguishes connected wallets with no seeded vault position", () => {
    const model = buildDashboardModel(dashboardSnapshot, {
      wallet: {
        address: "GEMPTYWALLET",
        status: "connected",
        label: "GEMPTYW...WALLET",
      },
      injectedPosition: {
        status: "not_found",
        error: "position not found",
      },
    });

    assert.equal(model.positionMode, "no wallet position");
    assert.equal(model.positionSource.detail, "showing seeded testnet evidence");
    assert.equal(
      model.readiness.find((item) => item.label === "Position Adapter").state,
      "wallet empty"
    );
  });
});
