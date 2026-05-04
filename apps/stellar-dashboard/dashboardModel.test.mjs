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
      model.readiness.find((item) => item.label === "Freighter Dashboard").state,
      "needs live verification"
    );
  });

  it("formats institutional dashboard values consistently", () => {
    assert.equal(formatCurrency(100000), "$100,000");
    assert.equal(formatPercent(0.00012), "0.01%");
  });
});
