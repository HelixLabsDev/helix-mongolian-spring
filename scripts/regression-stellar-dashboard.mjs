#!/usr/bin/env node

import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dashboardPath = "/apps/stellar-dashboard/";
const chromePath =
  process.env.CHROME_BIN || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

const cases = [
  {
    name: "freighter unavailable load",
    shim: browserShim({ wallet: "unavailable" }),
    async run(page, url) {
      await navigate(page, `${url}?case=unavailable#position`);
      await waitFor(page, () => document.querySelector("#connect-wallet")?.textContent === "Freighter unavailable", "Freighter unavailable CTA");
      const state = await pageState(page);
      assertEqual(state.walletStatus, "unavailable", "wallet status");
      assertEqual(state.positionSource, "static evidence", "position source");
      assertEqual(state.positionDetail, "seeded testnet evidence", "position detail");
      assertEqual(state.rpcStatus, "healthy", "RPC status");
    },
  },
  {
    name: "connected wallet fallback and navigation",
    shim: browserShim({ wallet: "connected", position: "fallback" }),
    async run(page, url) {
      await navigate(page, `${url}?case=connected-fallback#position`);
      await waitFor(page, () => document.querySelector("#connect-wallet")?.textContent === "GCI66ISW...3DDABB", "connected Freighter CTA");
      await assertConnectedFallback(page);

      await page.send("Page.reload", { ignoreCache: true });
      await waitFor(page, () => document.querySelector("#connect-wallet")?.textContent === "GCI66ISW...3DDABB", "connected Freighter after reload");
      await assertConnectedFallback(page);

      await clickHashLink(page, "activity");
      await waitFor(
        page,
        () => {
          const activityTop = document.getElementById("activity")?.getBoundingClientRect().top ?? Number.POSITIVE_INFINITY;
          return location.hash === "#activity" && window.scrollY > 300 && activityTop > 70 && activityTop < 520;
        },
        "Activity hash scroll"
      );

      await clickHashLink(page, "position");
      await waitFor(
        page,
        () => {
          const positionTop = document.getElementById("position")?.getBoundingClientRect().top ?? Number.POSITIVE_INFINITY;
          return location.hash === "#position" && positionTop >= 90 && positionTop < 180;
        },
        "Position hash scroll"
      );
    },
  },
  {
    name: "connected wallet no position state",
    shim: browserShim({ wallet: "connected", position: "not_found" }),
    async run(page, url) {
      await navigate(page, `${url}?case=no-position#position`);
      await waitFor(page, () => document.body.innerText.includes("no wallet position"), "no wallet position state");
      const state = await pageState(page);
      assertEqual(state.walletStatus, "connected", "wallet status");
      assertEqual(state.positionSource, "no wallet position", "position source");
      assertEqual(state.positionDetail, "showing seeded testnet evidence", "position detail");
      assertEqual(state.positionAdapter, "wallet empty", "position adapter gate");
    },
  },
];

async function main() {
  const server = await startDashboardServer();
  try {
    for (const testCase of cases) {
      await runCase(testCase, server.url);
      console.log(`ok - ${testCase.name}`);
    }
    console.log(`stellar dashboard browser regression: ${cases.length}/${cases.length} passed`);
  } finally {
    await server.close();
  }
}

async function runCase(testCase, baseUrl) {
  const browser = await startChrome(testCase.shim);
  try {
    await testCase.run(browser.page, baseUrl);
  } finally {
    await browser.close();
  }
}

async function assertConnectedFallback(page) {
  const state = await pageState(page);
  assertEqual(state.walletStatus, "connected", "wallet status");
  assertEqual(state.positionSource, "wallet fallback", "position source");
  assertEqual(state.positionDetail, "live wallet read unavailable", "position detail");
  assertEqual(state.freighterGate, "connected", "Freighter dashboard gate");
  assertEqual(state.positionAdapter, "fallback", "position adapter gate");
  assertEqual(state.rpcStatus, "healthy", "RPC status");
}

async function clickHashLink(page, targetId) {
  await page.evaluate((id) => {
    document.querySelector(`.sidebar a[href="#${id}"]`)?.click();
    return true;
  }, targetId);
}

async function navigate(page, url) {
  await page.send("Page.navigate", { url });
  await waitFor(page, () => document.readyState === "complete" && Boolean(document.querySelector("#connect-wallet")), "dashboard loaded");
}

async function pageState(page) {
  return page.evaluate(() => {
    const metricValue = (label) => {
      const metric = Array.from(document.querySelectorAll(".metric")).find((item) => item.querySelector(".metric-label")?.textContent === label);
      return {
        value: metric?.querySelector("strong")?.textContent?.trim() || null,
        detail: metric?.querySelector(".metric-detail")?.textContent?.trim() || null,
      };
    };
    const readinessValue = (label) => {
      const row = Array.from(document.querySelectorAll(".readiness-list > div")).find((item) => item.querySelector("span")?.textContent === label);
      return row?.querySelector("strong")?.textContent?.trim() || null;
    };

    return {
      cta: document.querySelector("#connect-wallet")?.textContent?.trim() || null,
      hash: location.hash,
      scrollY: window.scrollY,
      dataMode: metricValue("Data Mode").value,
      dataModeDetail: metricValue("Data Mode").detail,
      rpcStatus: metricValue("RPC").value,
      walletStatus: metricValue("Wallet").value,
      positionSource: metricValue("Position Source").value,
      positionDetail: metricValue("Position Source").detail,
      freighterGate: readinessValue("Freighter Dashboard"),
      positionAdapter: readinessValue("Position Adapter"),
    };
  });
}

async function waitFor(page, predicate, label, timeoutMs = 6000) {
  const started = Date.now();
  let lastError;
  while (Date.now() - started < timeoutMs) {
    try {
      if (await page.evaluate(predicate)) {
        return;
      }
    } catch (error) {
      lastError = error;
    }
    await delay(100);
  }
  const state = await pageState(page).catch(() => null);
  throw new Error(`${label} timed out${lastError ? `: ${lastError.message}` : ""}${state ? `\nState: ${JSON.stringify(state)}` : ""}`);
}

function browserShim({ wallet, position = "fallback" }) {
  const walletAddress = "GCI66ISWQ7M7BUU4WRAPYBLMVFGBBSR2Q24NBHQDEYC5EVXVM3DDABB";
  return `
    (() => {
      const appendChild = Element.prototype.appendChild;
      Element.prototype.appendChild = function(child) {
        const src = String(child?.src || "");
        if (child?.tagName === "SCRIPT" && src.includes("cdn.jsdelivr.net")) {
          setTimeout(() => child.onerror?.(new Error("blocked by regression shim")), 0);
          return child;
        }
        return appendChild.call(this, child);
      };

      window.fetch = async (_url, request = {}) => {
        const body = request.body ? JSON.parse(request.body) : {};
        const result = body.method === "getLatestLedger"
          ? { sequence: 2383001, protocolVersion: 22 }
          : { status: "healthy" };
        return {
          ok: true,
          status: 200,
          async json() {
            return { jsonrpc: "2.0", id: body.id || 1, result };
          },
        };
      };

      ${wallet === "connected" ? `
        window.freighterApi = {
          getAddress: async () => "${walletAddress}",
          getPublicKey: async () => "${walletAddress}",
          requestAccess: async () => ({ publicKey: "${walletAddress}" }),
        };
        window.helixStellarContracts = {
          vault: {
            contractId: "CAGG2XJJJGTER3E5BP26FVI3QLT4QYKZT233ZSPUC5O573QRU3D2Y7TW",
            getPositionSnapshot: async () => {
              throw new Error("${position === "not_found" ? "position not found" : "contract read failed"}");
            },
          },
          token: {
            assetsForShares: async () => "400000000",
            symbol: async (tokenAddress) => tokenAddress === "CBZVPTWMSPYJQ2UMUHWQVSVIZLAN72GGOZ33E77TIPVO5NO6QHLNZBGJ" ? "hstETH" : "XLM",
            decimals: async () => "7",
          },
          oracle: {
            lastprice: async () => ["25000000000", "1700000000"],
            decimals: async () => "7",
          },
        };
      ` : ""}
    })();
  `;
}

async function startDashboardServer() {
  const port = await freePort();
  const child = spawn(process.execPath, ["scripts/serve-stellar-dashboard.mjs"], {
    cwd: root,
    env: { ...process.env, PORT: String(port) },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const logs = collectOutput(child);
  const url = `http://127.0.0.1:${port}${dashboardPath}`;
  await waitForUrl(url, `dashboard server failed to start\n${logs.text}`);

  return {
    url,
    async close() {
      child.kill();
      await onceExit(child);
    },
  };
}

async function startChrome(initScript) {
  const debugPort = await freePort();
  const userDataDir = await mkdtemp(resolve(tmpdir(), "helix-dashboard-chrome-"));
  const child = spawn(chromePath, [
    "--headless=new",
    "--disable-gpu",
    "--disable-extensions",
    "--no-first-run",
    "--no-default-browser-check",
    "--window-size=1440,1100",
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${userDataDir}`,
    "about:blank",
  ], {
    stdio: ["ignore", "pipe", "pipe"],
  });
  const logs = collectOutput(child);

  try {
    const version = await waitForJson(`http://127.0.0.1:${debugPort}/json/version`, `Chrome did not expose DevTools\n${logs.text}`);
    let targets = await waitForJson(`http://127.0.0.1:${debugPort}/json/list`, `Chrome target list unavailable\n${logs.text}`);
    let target = targets.find((item) => item.type === "page" && item.webSocketDebuggerUrl);
    if (!target) {
      const created = await fetch(`http://127.0.0.1:${debugPort}/json/new?about:blank`, { method: "PUT" });
      target = await created.json();
    }

    const page = await CdpPage.connect(target.webSocketDebuggerUrl || version.webSocketDebuggerUrl);
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: initScript });

    return {
      page,
      async close() {
        await page.close();
        child.kill();
        await onceExit(child);
        await rm(userDataDir, { recursive: true, force: true });
      },
    };
  } catch (error) {
    child.kill();
    await onceExit(child);
    await rm(userDataDir, { recursive: true, force: true });
    throw error;
  }
}

class CdpPage {
  static async connect(url) {
    const ws = new WebSocket(url);
    const page = new CdpPage(ws);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    return page;
  }

  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id || !this.pending.has(message.id)) {
        return;
      }
      const { resolve: resolvePending, reject } = this.pending.get(message.id);
      this.pending.delete(message.id);
      if (message.error) {
        reject(new Error(message.error.message));
      } else {
        resolvePending(message.result);
      }
    });
  }

  async send(method, params = {}) {
    const id = ++this.id;
    const promise = new Promise((resolvePending, reject) => {
      this.pending.set(id, { resolve: resolvePending, reject });
    });
    this.ws.send(JSON.stringify({ id, method, params }));
    return promise;
  }

  async evaluate(fn, ...args) {
    const expression = `(${fn.toString()})(...${JSON.stringify(args)})`;
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.text || "Runtime evaluation failed");
    }
    return result.result.value;
  }

  async close() {
    this.ws.close();
  }
}

async function waitForUrl(url, label, timeoutMs = 6000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return response;
      }
    } catch {
      // Keep waiting.
    }
    await delay(100);
  }
  throw new Error(label);
}

async function waitForJson(url, label, timeoutMs = 6000) {
  const response = await waitForUrl(url, label, timeoutMs);
  return response.json();
}

async function freePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => resolvePort(address.port));
    });
    server.on("error", reject);
  });
}

function collectOutput(child) {
  const chunks = [];
  const append = (chunk) => {
    chunks.push(chunk.toString());
    if (chunks.length > 40) {
      chunks.shift();
    }
  };
  child.stdout.on("data", append);
  child.stderr.on("data", append);
  return {
    get text() {
      return chunks.join("");
    },
  };
}

async function onceExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  await new Promise((resolveExit) => child.once("exit", resolveExit));
}

function assertEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
