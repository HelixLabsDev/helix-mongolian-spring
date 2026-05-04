import {
  createStellarPositionReader,
  readWalletAddress,
} from "../../../product-adapters/stellar-position/reader.mjs";
import { mapStellarVaultSnapshotToHelixPosition } from "../../../product-adapters/stellar-position/adapter.mjs";

const DEFAULT_RPC_URL = "https://soroban-testnet.stellar.org";
const RPC_STORAGE_KEY = "helix-stellar-rpc-url";

export function resolveRpcUrl({ location, storage } = {}) {
  const params = new URLSearchParams(location?.search || "");
  const fromQuery = params.get("rpc");
  if (fromQuery) {
    return fromQuery;
  }

  try {
    return storage?.getItem(RPC_STORAGE_KEY) || DEFAULT_RPC_URL;
  } catch {
    return DEFAULT_RPC_URL;
  }
}

export function getFreighterApi(globalObject = globalThis) {
  return globalObject.freighterApi || globalObject.freighter || null;
}

export async function readFreighterWallet(globalObject = globalThis, { requestAccess = true } = {}) {
  const api = getFreighterApi(globalObject);
  if (!api) {
    return {
      address: null,
      status: "unavailable",
      label: "Freighter unavailable",
    };
  }

  try {
    const address = await readFreighterAddress(api, { requestAccess });
    return {
      address,
      status: "connected",
      label: shortAddress(address),
    };
  } catch (error) {
    if (!requestAccess) {
      return {
        address: null,
        status: "available",
        label: "Connect Freighter",
        error: error instanceof Error ? error.message : String(error),
      };
    }

    return {
      address: null,
      status: "blocked",
      label: "Wallet blocked",
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export async function callRpc({ rpcUrl, method, params, fetchImpl = globalThis.fetch }) {
  if (typeof fetchImpl !== "function") {
    throw new TypeError("fetch implementation is required");
  }

  const response = await fetchImpl(rpcUrl, {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method,
      params,
    }),
  });

  if (!response.ok) {
    throw new Error(`RPC ${method} failed with HTTP ${response.status}`);
  }

  const body = await response.json();
  if (body.error) {
    throw new Error(body.error.message || `RPC ${method} returned an error`);
  }

  return body.result;
}

export async function readRpcStatus({ rpcUrl = DEFAULT_RPC_URL, fetchImpl = globalThis.fetch } = {}) {
  try {
    const [health, latestLedger] = await Promise.all([
      callRpc({ rpcUrl, method: "getHealth", fetchImpl }),
      callRpc({ rpcUrl, method: "getLatestLedger", fetchImpl }),
    ]);

    return {
      rpcUrl,
      status: normalizeHealth(health),
      latestLedger: normalizeLedger(latestLedger),
      live: true,
    };
  } catch (error) {
    return {
      rpcUrl,
      status: "unavailable",
      latestLedger: null,
      live: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export async function readInjectedPosition({
  globalObject = globalThis,
  wallet,
  debtPriceUsd = 1,
  useGlobalWalletFallback = true,
} = {}) {
  const contracts = globalObject.helixStellarContracts;
  if (!contracts) {
    return null;
  }
  const walletClient = wallet || (useGlobalWalletFallback ? getFreighterApi(globalObject) : null);
  if (!walletClient) {
    return {
      snapshot: null,
      position: null,
      status: "unavailable",
      error: "wallet unavailable",
    };
  }

  const reader = createStellarPositionReader({
    wallet: walletClient,
    contracts,
    debtPriceUsd,
  });

  try {
    const snapshot = await reader.readSnapshot();
    return {
      snapshot,
      position: mapStellarVaultSnapshotToHelixPosition(snapshot),
      status: "live",
    };
  } catch (error) {
    return {
      snapshot: null,
      position: null,
      status: "unavailable",
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export async function readLiveDashboardInputs({
  globalObject = globalThis,
  fetchImpl = globalThis.fetch,
  location = globalObject.location,
  storage = globalObject.localStorage,
} = {}) {
  const rpcUrl = resolveRpcUrl({ location, storage });
  const [wallet, rpc] = await Promise.all([
    readFreighterWallet(globalObject, { requestAccess: false }),
    readRpcStatus({ rpcUrl, fetchImpl }),
  ]);
  const injectedPosition = await readInjectedPosition({
    globalObject,
    wallet: wallet.address ? { getPublicKey: async () => wallet.address } : null,
    useGlobalWalletFallback: false,
  });

  return {
    wallet,
    rpc,
    injectedPosition,
  };
}

export function shortAddress(address, head = 8, tail = 6) {
  if (!address || address.length <= head + tail + 3) {
    return address || "";
  }
  return `${address.slice(0, head)}...${address.slice(-tail)}`;
}

async function readFreighterAddress(api, { requestAccess }) {
  if (requestAccess && typeof api.requestAccess === "function") {
    return normalizeWalletAddress(await api.requestAccess(), "wallet.requestAccess()");
  }
  if (!requestAccess && typeof api.getAddress === "function") {
    return normalizeWalletAddress(await api.getAddress(), "wallet.getAddress()");
  }
  if (!requestAccess && typeof api.getPublicKey === "function") {
    return normalizeWalletAddress(await api.getPublicKey(), "wallet.getPublicKey()");
  }
  if (!requestAccess) {
    throw new TypeError("wallet has no passive address method");
  }
  return readWalletAddress(api);
}

function normalizeWalletAddress(value, fieldName) {
  if (typeof value === "string" && value.length > 0) {
    return value;
  }
  if (value && typeof value === "object") {
    const address = value.address ?? value.publicKey;
    if (typeof address === "string" && address.length > 0) {
      return address;
    }
  }
  throw new TypeError(`${fieldName} must return an address string`);
}

function normalizeHealth(value) {
  if (typeof value === "string") {
    return value;
  }
  if (value && typeof value.status === "string") {
    return value.status;
  }
  return "unknown";
}

function normalizeLedger(value) {
  if (!value || typeof value !== "object") {
    return null;
  }
  return {
    sequence: value.sequence ?? value.id ?? value.latestLedger ?? null,
    protocolVersion: value.protocolVersion ?? null,
  };
}
