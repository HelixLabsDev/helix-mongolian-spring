import {
  buildDashboardModel,
  dashboardSnapshot,
  formatCurrency,
  formatNumber,
  formatPercent,
} from "./dashboardModel.mjs";
import {
  readFreighterWallet,
  readInjectedPosition,
  readLiveDashboardInputs,
  shortAddress as shortLiveAddress,
} from "./liveData.mjs";
import { installHelixStellarContracts } from "./contractFacades.mjs";

const app = document.querySelector("#app");
const NAV_TARGETS = new Set(["position", "bridge", "risk", "blend", "activity"]);
let lastHandledHash = window.location.hash;
let liveInputs = {};
let model = buildDashboardModel(dashboardSnapshot, liveInputs);

if ("scrollRestoration" in window.history) {
  window.history.scrollRestoration = "manual";
}

function shortHash(value, head = 8, tail = 6) {
  if (!value || value.length <= head + tail + 3) return value;
  return `${value.slice(0, head)}...${value.slice(-tail)}`;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function statusClass(value) {
  const normalized = String(value).toLowerCase();
  if (["complete", "connected", "executed", "green", "healthy", "live", "trusted", "upgraded", "ready", "clear", "current"].includes(normalized)) {
    return "status-good";
  }
  if (normalized.includes("needs") || normalized.includes("not deployed")) {
    return "status-warn";
  }
  return "status-neutral";
}

function displayStatus(value) {
  return String(value).replaceAll("_", " ");
}

function metric(label, value, detail = "") {
  return `
    <div class="metric">
      <span class="metric-label">${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
      ${detail ? `<span class="metric-detail">${escapeHtml(detail)}</span>` : ""}
    </div>
  `;
}

function panel(title, body) {
  return `
    <section class="panel">
      <header class="panel-header">
        <h2>${title}</h2>
      </header>
      <div class="panel-body">${body}</div>
    </section>
  `;
}

function renderTicker() {
  return `
    <div class="ticker" aria-label="Stellar terminal metrics">
      ${model.ticker
        .map(
          (item) => `
            <div class="ticker-item">
              <span>${escapeHtml(item.label)}</span>
              <strong>${escapeHtml(item.value)}</strong>
            </div>
          `
        )
        .join("")}
    </div>
  `;
}

function renderHeader(walletLabel = model.wallet.label || "Connect Freighter") {
  return `
    <header class="topbar">
      <div class="brand-row">
        <div class="brand-mark" aria-hidden="true">H</div>
        <div>
          <h1>Helix</h1>
          <span>Stellar Terminal</span>
        </div>
      </div>
      <div class="topbar-actions">
        <span class="network">${escapeHtml(model.network)}</span>
        <button id="connect-wallet" type="button">${escapeHtml(walletLabel)}</button>
      </div>
    </header>
  `;
}

function renderPositionPanel() {
  const position = model.position;
  return panel(
    "Vault Position",
    `
      <div class="metric-grid two">
        ${metric("Collateral", `${formatNumber(position.collateral_amount)} ${position.collateral_token}`, formatCurrency(position.collateral_value_usd))}
        ${metric("Debt", `${formatNumber(position.debt_amount)} ${position.debt_token}`, formatCurrency(position.debt_value_usd))}
        ${metric("Health Factor", position.health_factor.toFixed(4), position.status)}
        ${metric("Borrow Rate", formatPercent(position.borrow_rate), "current pool config")}
        ${metric("Data Mode", model.positionSource.mode, model.positionSource.detail)}
      </div>
      <div class="bar-block">
        <div class="bar-row">
          <span>LTV</span>
          <strong>${formatPercent(position.ltv_current)}</strong>
        </div>
        <div class="bar-track">
          <div class="bar-fill" style="width: ${Math.min(model.risk.utilization * 100, 100).toFixed(2)}%"></div>
        </div>
        <div class="bar-row muted">
          <span>Max ${formatPercent(position.ltv_max)}</span>
          <span>Liquidation ${formatPercent(position.liquidation_threshold)}</span>
        </div>
      </div>
    `
  );
}

function renderNetworkPanel() {
  const ledger = model.rpc.latestLedger?.sequence ?? "unavailable";
  const rpcUrl = model.rpc.rpcUrl ? model.rpc.rpcUrl.replace(/^https?:\/\//, "") : "not configured";
  return panel(
    "Network Status",
    `
      <div class="metric-grid two">
        ${metric("RPC", model.rpc.status, rpcUrl)}
        ${metric("Latest Ledger", ledger, model.rpc.live ? "live" : "not live")}
        ${metric("Wallet", displayStatus(model.wallet.status), model.wallet.address ? shortLiveAddress(model.wallet.address) : "not connected")}
        ${metric("Position Source", model.positionSource.mode, model.positionSource.detail)}
      </div>
    `
  );
}

function renderBridgePanel() {
  return panel(
    "Bridge Proof",
    `
      <div class="proof-grid">
        <div>
          <span class="label">Sepolia Source</span>
          <code>${shortHash(model.bridgeProof.sourceTx, 12, 8)}</code>
        </div>
        <div>
          <span class="label">Stellar Execute</span>
          <code>${shortHash(model.bridgeProof.stellarTx, 12, 8)}</code>
        </div>
        <div>
          <span class="label">Ledger</span>
          <strong>${model.bridgeProof.stellarLedger}</strong>
        </div>
        <div>
          <span class="label">Amount</span>
          <strong>${model.bridgeProof.displayAmount}</strong>
        </div>
        <div>
          <span class="label">Status</span>
          <strong class="${statusClass(model.bridgeProof.status)}">${model.bridgeProof.status}</strong>
        </div>
      </div>
    `
  );
}

function renderRiskPanel() {
  return panel(
    "Liquidation Risk",
    `
      <div class="metric-grid two">
        ${metric("Buffer", formatPercent(model.risk.bufferToLiquidation), "to liquidation threshold")}
        ${metric("Oracle", model.risk.oracleFreshness, "safe mode clear")}
      </div>
      <div class="risk-band">
        <span>Stable</span>
        <strong>${model.risk.safeMode}</strong>
      </div>
    `
  );
}

function renderBlendPanel() {
  return panel(
    "Blend Readiness",
    `
      <div class="readiness-list">
        <div><span>Adaptor</span><strong>${model.blend.adaptor}</strong></div>
        <div><span>Unit Tests</span><strong>${model.blend.unitTests}</strong></div>
        <div><span>Smoke</span><strong class="status-good">${model.blend.smoke}</strong></div>
        <div><span>Pool</span><strong class="status-warn">${model.blend.poolDeployment}</strong></div>
      </div>
    `
  );
}

function renderContractsPanel() {
  return panel(
    "Contract Surface",
    `
      <div class="contract-list">
        ${model.contracts
          .map(
            (contract) => `
              <div class="contract-row">
                <div>
                  <span>${contract.label}</span>
                  <code>${shortHash(contract.value, 14, 8)}</code>
                </div>
                <strong class="${statusClass(contract.status)}">${contract.status}</strong>
              </div>
            `
          )
          .join("")}
      </div>
    `
  );
}

function renderActivityPanel() {
  return panel(
    "Activity",
    `
      <div class="timeline">
        ${model.events
          .map(
            (event) => `
              <div class="timeline-row">
                <time>${event.time}</time>
                <div>
                  <strong>${event.label}</strong>
                  <span>${event.detail}</span>
                </div>
              </div>
            `
          )
          .join("")}
      </div>
    `
  );
}

function renderReadinessPanel() {
  return panel(
    "T3 Gate",
    `
      <div class="readiness-list">
        ${model.readiness
          .map(
            (item) => `
              <div>
                <span>${item.label}</span>
                <strong class="${statusClass(item.state)}">${item.state}</strong>
              </div>
            `
          )
          .join("")}
      </div>
    `
  );
}

function renderInstallBanner() {
  if (model.wallet.status !== "not_installed") return "";
  const url = model.wallet.installUrl || "https://www.freighter.app";
  return `
    <div class="install-banner" role="alert">
      <strong>Freighter wallet not detected.</strong>
      <span>You need the Freighter browser extension to connect. It's free and takes about 30 seconds.</span>
      <a href="${escapeHtml(url)}" target="_blank" rel="noopener noreferrer" class="install-link">Install Freighter ↗</a>
    </div>
  `;
}

function render(walletLabel) {
  app.innerHTML = `
    ${renderHeader(walletLabel)}
    ${renderInstallBanner()}
    ${renderTicker()}
    <main class="layout">
      <aside class="sidebar">
        <nav>
          <a href="#position">Position</a>
          <a href="#bridge">Bridge</a>
          <a href="#risk">Risk</a>
          <a href="#blend">Blend</a>
          <a href="#activity">Activity</a>
        </nav>
      </aside>
      <section class="dashboard">
        <div id="position">${renderPositionPanel()}</div>
        <div id="bridge">${renderBridgePanel()}</div>
        <div id="risk">${renderRiskPanel()}</div>
        <div>${renderNetworkPanel()}</div>
        <div id="blend">${renderBlendPanel()}</div>
        <div>${renderContractsPanel()}</div>
        <div id="activity">${renderActivityPanel()}</div>
        <div>${renderReadinessPanel()}</div>
      </section>
    </main>
  `;

  document.querySelector("#connect-wallet")?.addEventListener("click", connectFreighter);
  bindNavigationHandlers();
  scheduleScrollToCurrentHash();
}

function bindNavigationHandlers() {
  document.querySelectorAll(".sidebar a[href^='#']").forEach((link) => {
    link.addEventListener("click", (event) => {
      const targetId = link.getAttribute("href")?.slice(1);
      if (!NAV_TARGETS.has(targetId)) {
        return;
      }

      event.preventDefault();
      scrollToSection(targetId);
    });
  });
}

function scheduleScrollToCurrentHash() {
  lastHandledHash = window.location.hash;
  const targetId = window.location.hash.slice(1);
  if (!NAV_TARGETS.has(targetId)) {
    return;
  }

  const scroll = () => scrollToSection(targetId, { updateHash: false, behavior: "auto" });
  if (typeof window.requestAnimationFrame === "function") {
    window.requestAnimationFrame(() => window.requestAnimationFrame(scroll));
    setTimeout(scroll, 150);
    setTimeout(scroll, 500);
    return;
  }

  setTimeout(scroll, 0);
}

function scrollToSection(targetId, { updateHash = true, behavior = "smooth" } = {}) {
  const target = document.getElementById(targetId);
  if (!target) {
    return;
  }

  const topbarHeight = document.querySelector(".topbar")?.offsetHeight || 0;
  const tickerHeight = document.querySelector(".ticker")?.offsetHeight || 0;
  const topOffset = topbarHeight + tickerHeight + 10;
  target.scrollIntoView({ behavior, block: "start" });
  window.scrollBy({ top: -topOffset, behavior });

  if (updateHash && window.location.hash !== `#${targetId}`) {
    window.history.pushState(null, "", `#${targetId}`);
  }
}

function syncHashScroll() {
  if (window.location.hash !== lastHandledHash) {
    scheduleScrollToCurrentHash();
  }
}

window.addEventListener("hashchange", scheduleScrollToCurrentHash);
window.addEventListener("popstate", scheduleScrollToCurrentHash);
setInterval(syncHashScroll, 250);

async function connectFreighter() {
  const wallet = await readFreighterWallet(window);
  if (wallet.status === "not_installed" && wallet.installUrl) {
    window.open(wallet.installUrl, "_blank", "noopener,noreferrer");
  }
  liveInputs = {
    ...liveInputs,
    wallet,
    injectedPosition: await readInjectedPosition({
      globalObject: window,
      wallet: wallet.address ? { getPublicKey: async () => wallet.address } : null,
      useGlobalWalletFallback: false,
    }),
  };
  model = buildDashboardModel(dashboardSnapshot, liveInputs);
  render();
}

render();

installHelixStellarContracts(window)
  .catch(() => null)
  .then(() => readLiveDashboardInputs({ globalObject: window }))
  .then((inputs) => {
    liveInputs = inputs;
    model = buildDashboardModel(dashboardSnapshot, liveInputs);
    render();
  })
  .catch(() => {
    render();
  });
