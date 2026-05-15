import { mapStellarVaultSnapshotToHelixPosition } from "../../../product-adapters/stellar-position/adapter.mjs";

const BRIDGE_RAW_AMOUNT = "10000000000000000";

export const dashboardSnapshot = {
  userAddress: "GD7DWCBNE4PK5OLGFJCXV7ZJ6A7YK4FBIYNS7JIGYFQKCZAA76ZP7PGS",
  vaultContractId: "CAGG2XJJJGTER3E5BP26FVI3QLT4QYKZT233ZSPUC5O573QRU3D2Y7TW",
  position: {
    deposited_shares: "400000000",
    borrowed_amount: "120000000",
    last_update: "1777916303",
  },
  poolConfig: {
    max_ltv: "7500",
    liq_threshold: "8000",
    interest_rate: "500",
  },
  collateral: {
    tokenAddress: "CBZVPTWMSPYJQ2UMUHWQVSVIZLAN72GGOZ33E77TIPVO5NO6QHLNZBGJ",
    symbol: "hstETH",
    decimals: "7",
    assets: "400000000",
  },
  debt: {
    tokenAddress: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
    symbol: "XLM",
    decimals: "7",
  },
  prices: {
    collateralUsd: 2500,
    debtUsd: 1,
  },
  healthFactorBps: "66666665",
  accruedInterest: "2",
  status: "active",
};

const bridgeProof = {
  sourceTx: "0xeb1863825a0d73c0cc67eae3e3fb5edb574ab615ebf002c0645c3eefbd7a3fb9",
  stellarTx: "0e55c7d53ab84a1953b68eef0913b05f00870d05285b4e00847342f9a8f3dce6",
  stellarLedger: "2379754",
  executedAt: "2026-05-04T14:49:46Z",
  rawAmount: BRIDGE_RAW_AMOUNT,
  displayAmount: "0.01 WETH",
  status: "executed",
};

const contracts = [
  {
    label: "hstETH Token",
    value: "CBZVPTWMSPYJQ2UMUHWQVSVIZLAN72GGOZ33E77TIPVO5NO6QHLNZBGJ",
    status: "live",
  },
  {
    label: "Bridge hstETH Token",
    value: "CC366YM6MJOISQSCUXBU3BCRNKVCDI7VOZT3SF7AJKL7ILTMXY3AGBJ2",
    status: "executed",
  },
  {
    label: "Price Oracle",
    value: "CBBEWFLE2X27FGNENDG5IXJS5LDUHDIIDS6W6XPJN4F5VUNSCJVLSRMD",
    status: "live",
  },
  {
    label: "Bridge Handler",
    value: "CCBI7ZKMKOEHUCOLBXW63QKMFN5MFIDANODW5L4IO4RC5XPCD2IEDTQY",
    status: "live",
  },
  {
    label: "Collateral Vault",
    value: "CAGG2XJJJGTER3E5BP26FVI3QLT4QYKZT233ZSPUC5O573QRU3D2Y7TW",
    status: "live",
  },
  {
    label: "Axelar Migrator",
    value: "0x5A33F35f4B02269107e60713bc2dAb970C741a0c",
    status: "trusted",
  },
];

const events = [
  {
    time: "17:38:23Z",
    label: "Vault position seeded",
    detail: "helix-deployer deposited 40 hstETH and borrowed 12 XLM",
  },
  {
    time: "17:32:00Z",
    label: "Vault snapshot interface live",
    detail: "Collateral Vault upgraded to expose get_position_snapshot",
  },
  {
    time: "14:49:46Z",
    label: "GMP replay executed",
    detail: "gateway message_executed -> token mint -> deposit_processed",
  },
  {
    time: "14:36:00Z",
    label: "Source config repaired",
    detail: "ethereum-sepolia + AxelarMigrator v2 stored without shell quotes",
  },
  {
    time: "14:31:00Z",
    label: "Live token upgraded",
    detail: "hstETH interface exposes bridge_mint and bridge_burn",
  },
  {
    time: "14:21:00Z",
    label: "Sepolia migrate landed",
    detail: "0.01 WETH routed through Axelar GMP",
  },
];

export function buildDashboardModel(snapshot = dashboardSnapshot, liveInputs = {}) {
  const injected = liveInputs.injectedPosition;
  const activeSnapshot = injected?.snapshot || snapshot;
  const position = injected?.position || mapStellarVaultSnapshotToHelixPosition(activeSnapshot);
  const utilization = position.ltv_max === 0 ? 0 : position.ltv_current / position.ltv_max;
  const bufferToLiquidation = position.liquidation_threshold - position.ltv_current;
  const wallet = liveInputs.wallet || {
    address: activeSnapshot.userAddress,
    status: "static",
    label: "Connect Freighter",
  };
  const rpc = liveInputs.rpc || {
    rpcUrl: null,
    status: "static",
    latestLedger: null,
    live: false,
  };
  const positionSource = resolvePositionSource({ injected, wallet });
  const positionMode = positionSource.mode;

  return {
    network: "stellar-2026-q1-2",
    wallet: {
      address: wallet.address || (wallet.status === "static" ? activeSnapshot.userAddress : null),
      status: wallet.status,
      label: wallet.label,
      installUrl: wallet.installUrl || null,
    },
    rpc,
    positionMode,
    positionSource,
    position,
    bridgeProof,
    contracts,
    events,
    risk: {
      utilization,
      bufferToLiquidation,
      safeMode: "clear",
      oracleFreshness: "current",
    },
    blend: {
      adaptor: "helix-blend-oracle-adaptor",
      unitTests: "10/10",
      smoke: "real Blend pool smoke green",
      poolDeployment: "not deployed",
    },
    readiness: [
      { label: "Bridge E2E", state: "complete" },
      { label: "Liquidation Engine", state: "complete" },
      { label: "Position Adapter", state: positionSource.readinessState },
      { label: "Freighter Dashboard", state: wallet.status === "connected" ? "connected" : "needs live verification" },
      { label: "T3 Evidence Packet", state: "complete" },
    ],
    ticker: [
      { label: "Bridge Amount", value: bridgeProof.displayAmount },
      { label: "Health Factor", value: position.health_factor.toFixed(2) },
      { label: "LTV", value: formatPercent(position.ltv_current) },
      { label: "RPC", value: rpc.status },
      { label: "Blend Smoke", value: "Green" },
      { label: "Bridge", value: "Executed" },
    ],
  };
}

function resolvePositionSource({ injected, wallet }) {
  if (injected?.status === "live") {
    return {
      mode: "live",
      detail: "wallet contract facades",
      readinessState: "live",
    };
  }

  if (wallet.status === "connected" && injected?.status === "not_found") {
    return {
      mode: "no wallet position",
      detail: "showing seeded testnet evidence",
      readinessState: "wallet empty",
    };
  }

  if (wallet.status === "connected") {
    return {
      mode: "wallet fallback",
      detail: "live wallet read unavailable",
      readinessState: "fallback",
    };
  }

  return {
    mode: "static evidence",
    detail: "seeded testnet evidence",
    readinessState: "complete",
  };
}

export function formatCurrency(value) {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 0,
  }).format(value);
}

export function formatNumber(value, maximumFractionDigits = 2) {
  return new Intl.NumberFormat("en-US", {
    maximumFractionDigits,
  }).format(value);
}

export function formatPercent(value) {
  return `${(value * 100).toFixed(2)}%`;
}
