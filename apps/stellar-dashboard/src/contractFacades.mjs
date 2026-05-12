import { dashboardSnapshot } from "./dashboardModel.mjs";

export const STELLAR_SDK_SCRIPT_URL =
  "https://cdn.jsdelivr.net/npm/@stellar/stellar-sdk@14.5.0/dist/stellar-sdk.min.js";

export const HELIX_STELLAR_CONTRACT_IDS = {
  vault: dashboardSnapshot.vaultContractId,
  token: dashboardSnapshot.collateral.tokenAddress,
  oracle: "CBBEWFLE2X27FGNENDG5IXJS5LDUHDIIDS6W6XPJN4F5VUNSCJVLSRMD",
};

const DEBT_TOKEN_METADATA = {
  [dashboardSnapshot.debt.tokenAddress]: {
    symbol: dashboardSnapshot.debt.symbol,
    decimals: dashboardSnapshot.debt.decimals,
  },
};

const VAULT_SPEC = [
  "AAAAAQAAAAAAAAAAAAAACFBvc2l0aW9uAAAAAwAAAAAAAAAPYm9ycm93ZWRfYW1vdW50AAAAAAsAAAAAAAAAEGRlcG9zaXRlZF9zaGFyZXMAAAALAAAAAAAAAAtsYXN0X3VwZGF0ZQAAAAAG",
  "AAAAAQAAAAAAAAAAAAAAClBvb2xDb25maWcAAAAAAAUAAAAAAAAADWludGVyZXN0X3JhdGUAAAAAAAAEAAAAAAAAAAlsaXFfYm9udXMAAAAAAAAEAAAAAAAAAA1saXFfdGhyZXNob2xkAAAAAAAABAAAAAAAAAAHbWF4X2x0dgAAAAAEAAAAAAAAAAxtaW5fcG9zaXRpb24AAAAL",
  "AAAAAQAAAAAAAAAAAAAAEFBvc2l0aW9uU25hcHNob3QAAAAHAAAAAAAAABBhY2NydWVkX2ludGVyZXN0AAAACwAAAAAAAAAQYWNjcnVlZF9wb3NpdGlvbgAAB9AAAAAIUG9zaXRpb24AAAAAAAAADGJvcnJvd190b2tlbgAAABMAAAAAAAAAEGNvbGxhdGVyYWxfdG9rZW4AAAATAAAAAAAAAA1oZWFsdGhfZmFjdG9yAAAAAAAACwAAAAAAAAALcG9vbF9jb25maWcAAAAH0AAAAApQb29sQ29uZmlnAAAAAAAAAAAAD3N0b3JlZF9wb3NpdGlvbgAAAAfQAAAACFBvc2l0aW9u",
  "AAAAAAAAAAAAAAAVZ2V0X3Bvc2l0aW9uX3NuYXBzaG90AAAAAAAAAQAAAAAAAAAEdXNlcgAAABMAAAABAAAH0AAAABBQb3NpdGlvblNuYXBzaG90",
];

const TOKEN_SPEC = [
  "AAAAAAAAAAAAAAAGc3ltYm9sAAAAAAAAAAAAAQAAABA=",
  "AAAAAAAAAAAAAAAIZGVjaW1hbHMAAAAAAAAAAQAAAAQ=",
  "AAAAAAAAAAAAAAARYXNzZXRzX2Zvcl9zaGFyZXMAAAAAAAABAAAAAAAAAAZzaGFyZXMAAAAAAAsAAAABAAAACw==",
];

const ORACLE_SPEC = [
  "AAAAAAAAAAAAAAAIZGVjaW1hbHMAAAABAAAAAAAAAAVhc3NldAAAAAAAABMAAAABAAAABA==",
  "AAAAAAAAAAAAAAAJbGFzdHByaWNlAAAAAAAAAQAAAAAAAAAFYXNzZXQAAAAAAAATAAAAAQAAA+0AAAACAAAACwAAAAY=",
];

export async function installHelixStellarContracts(globalObject = globalThis, options = {}) {
  if (globalObject.helixStellarContracts && !options.force) {
    return globalObject.helixStellarContracts;
  }

  const contracts = await createHelixStellarContracts({
    ...options,
    globalObject,
  });
  globalObject.helixStellarContracts = contracts;
  return contracts;
}

export async function createHelixStellarContracts({
  globalObject = globalThis,
  stellarSdk,
  rpcUrl = "https://soroban-testnet.stellar.org",
  networkPassphrase,
  contractIds = HELIX_STELLAR_CONTRACT_IDS,
  debtTokenMetadata = DEBT_TOKEN_METADATA,
  sdkScriptUrl = STELLAR_SDK_SCRIPT_URL,
} = {}) {
  const sdk = stellarSdk || (await loadStellarSdk(globalObject, sdkScriptUrl));
  const passphrase = networkPassphrase || sdk.Networks?.TESTNET || "Test SDF Network ; September 2015";
  const vaultClient = createClient(sdk, VAULT_SPEC, contractIds.vault, rpcUrl, passphrase);
  const tokenClient = createClient(sdk, TOKEN_SPEC, contractIds.token, rpcUrl, passphrase);
  const oracleClient = createClient(sdk, ORACLE_SPEC, contractIds.oracle, rpcUrl, passphrase);

  return {
    vault: {
      contractId: contractIds.vault,
      getPositionSnapshot(user) {
        return transactionResult(vaultClient.get_position_snapshot({ user }));
      },
      get_position_snapshot(user) {
        return this.getPositionSnapshot(user);
      },
    },
    token: {
      contractId: contractIds.token,
      assetsForShares(tokenAddress, shares) {
        assertKnownCollateralToken(contractIds.token, tokenAddress);
        return transactionResult(tokenClient.assets_for_shares({ shares: integerString(shares) }));
      },
      assets_for_shares(tokenAddress, shares) {
        return this.assetsForShares(tokenAddress, shares);
      },
      async symbol(tokenAddress) {
        if (sameAddress(contractIds.token, tokenAddress)) {
          return transactionResult(tokenClient.symbol());
        }
        return metadataValue(debtTokenMetadata, tokenAddress, "symbol");
      },
      async decimals(tokenAddress) {
        if (sameAddress(contractIds.token, tokenAddress)) {
          return transactionResult(tokenClient.decimals());
        }
        return metadataValue(debtTokenMetadata, tokenAddress, "decimals");
      },
    },
    oracle: {
      contractId: contractIds.oracle,
      lastprice(tokenAddress) {
        return transactionResult(oracleClient.lastprice({ asset: addressString(tokenAddress) }));
      },
      lastPrice(tokenAddress) {
        return this.lastprice(tokenAddress);
      },
      decimals(tokenAddress) {
        return transactionResult(oracleClient.decimals({ asset: addressString(tokenAddress) }));
      },
    },
  };
}

export async function loadStellarSdk(globalObject = globalThis, sdkScriptUrl = STELLAR_SDK_SCRIPT_URL) {
  if (globalObject.StellarSdk?.contract?.Client) {
    return globalObject.StellarSdk;
  }

  const document = globalObject.document;
  if (!document?.createElement) {
    throw new TypeError("Stellar SDK is required outside a browser context");
  }

  await new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = sdkScriptUrl;
    script.async = true;
    script.onload = resolve;
    script.onerror = () => reject(new Error("Stellar SDK failed to load"));
    document.head.appendChild(script);
  });

  if (!globalObject.StellarSdk?.contract?.Client) {
    throw new TypeError("Stellar SDK contract client is unavailable");
  }
  return globalObject.StellarSdk;
}

function createClient(sdk, specEntries, contractId, rpcUrl, networkPassphrase) {
  const { Client, Spec } = sdk.contract || {};
  if (typeof Client !== "function" || typeof Spec !== "function") {
    throw new TypeError("Stellar SDK contract client is unavailable");
  }

  return new Client(new Spec(specEntries), {
    contractId,
    rpcUrl,
    networkPassphrase,
  });
}

async function transactionResult(transactionPromise) {
  const transaction = await transactionPromise;
  if (transaction && "result" in Object(transaction)) {
    return transaction.result;
  }
  return transaction;
}

function metadataValue(metadata, tokenAddress, fieldName) {
  const entry = metadata[addressString(tokenAddress)];
  if (!entry || entry[fieldName] === undefined) {
    throw new TypeError(`No metadata configured for token ${addressString(tokenAddress)}`);
  }
  return entry[fieldName];
}

function assertKnownCollateralToken(expected, actual) {
  if (!sameAddress(expected, actual)) {
    throw new TypeError(`Unsupported collateral token ${addressString(actual)}`);
  }
}

function sameAddress(left, right) {
  return addressString(left) === addressString(right);
}

function addressString(value) {
  if (value && typeof value === "object" && typeof value.toString === "function") {
    return value.toString();
  }
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError("token address must be a non-empty string");
  }
  return value;
}

function integerString(value) {
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (typeof value === "number" && Number.isInteger(value)) {
    return value.toString();
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    return value;
  }
  throw new TypeError("integer string is required");
}
