import {
  mapStellarVaultSnapshotToHelixPosition,
  scaleIntegerToNumber,
} from "./adapter.mjs";

export function createStellarPositionReader(config) {
  const readerConfig = requireObject(config, "config");
  const wallet = requireObject(readerConfig.wallet, "wallet");
  const contracts = requireObject(readerConfig.contracts, "contracts");
  const vault = requireObject(contracts.vault, "contracts.vault");
  const token = requireObject(contracts.token, "contracts.token");
  const oracle = requireObject(contracts.oracle, "contracts.oracle");

  return {
    async readSnapshot() {
      return readStellarVaultSnapshot({
        wallet,
        vault,
        token,
        oracle,
        debtPriceUsd: readerConfig.debtPriceUsd ?? 1,
      });
    },

    async readPosition() {
      const snapshot = await this.readSnapshot();
      return mapStellarVaultSnapshotToHelixPosition(snapshot);
    },
  };
}

export async function readStellarVaultSnapshot({ wallet, vault, token, oracle, debtPriceUsd = 1 }) {
  const userAddress = await readWalletAddress(wallet);
  const vaultContractId = stringValue(readAny(vault, "contractId", "contract_id"), "vault.contractId");
  const positionSnapshot = normalizePositionSnapshot(
    await requiredMethod(vault, "getPositionSnapshot", "get_position_snapshot")(userAddress)
  );

  const position = positionSnapshot.accrued_position;
  const poolConfig = positionSnapshot.pool_config;
  const collateralTokenAddress = stringValue(positionSnapshot.collateral_token, "collateral_token");
  const borrowTokenAddress = stringValue(positionSnapshot.borrow_token, "borrow_token");
  const collateralAssets = await requiredMethod(token, "assetsForShares", "assets_for_shares")(
    collateralTokenAddress,
    position.deposited_shares
  );
  const [collateralSymbol, collateralDecimals, debtSymbol, debtDecimals, oraclePrice, oracleDecimals] =
    await Promise.all([
      requiredMethod(token, "symbol")(collateralTokenAddress),
      requiredMethod(token, "decimals")(collateralTokenAddress),
      requiredMethod(token, "symbol")(borrowTokenAddress),
      requiredMethod(token, "decimals")(borrowTokenAddress),
      requiredMethod(oracle, "lastprice", "lastPrice")(collateralTokenAddress),
      requiredMethod(oracle, "decimals")(collateralTokenAddress),
    ]);

  return {
    userAddress,
    vaultContractId,
    position,
    poolConfig,
    collateral: {
      tokenAddress: collateralTokenAddress,
      symbol: stringValue(collateralSymbol, "collateral.symbol"),
      decimals: integerValue(collateralDecimals, "collateral.decimals"),
      assets: integerValue(collateralAssets, "collateral.assets"),
    },
    debt: {
      tokenAddress: borrowTokenAddress,
      symbol: stringValue(debtSymbol, "debt.symbol"),
      decimals: integerValue(debtDecimals, "debt.decimals"),
    },
    prices: {
      collateralUsd: priceTupleToNumber(oraclePrice, oracleDecimals),
      debtUsd: numberValue(debtPriceUsd, "debtPriceUsd"),
    },
    healthFactorBps: positionSnapshot.health_factor,
    accruedInterest: positionSnapshot.accrued_interest,
    asOf: position.last_update,
  };
}

export async function readWalletAddress(wallet) {
  const walletClient = requireObject(wallet, "wallet");
  if (typeof walletClient.getPublicKey === "function") {
    return stringValue(await walletClient.getPublicKey(), "wallet.getPublicKey()");
  }
  if (typeof walletClient.getAddress === "function") {
    return stringValue(await walletClient.getAddress(), "wallet.getAddress()");
  }
  if (typeof walletClient.requestAccess === "function") {
    const access = await walletClient.requestAccess();
    if (typeof access === "string") {
      return stringValue(access, "wallet.requestAccess()");
    }
    const accessObject = requireObject(access, "wallet.requestAccess()");
    return stringValue(accessObject.address ?? accessObject.publicKey, "wallet.requestAccess().address");
  }
  throw new TypeError("wallet must expose getPublicKey(), getAddress(), or requestAccess()");
}

export function normalizePositionSnapshot(value) {
  const snapshot = requireObject(value, "positionSnapshot");
  const storedPosition = normalizePosition(readField(snapshot, "stored_position", "storedPosition"));
  const accruedPosition = normalizePosition(readField(snapshot, "accrued_position", "accruedPosition"));
  const poolConfig = normalizePoolConfig(readField(snapshot, "pool_config", "poolConfig"));

  return {
    stored_position: storedPosition,
    accrued_position: accruedPosition,
    accrued_interest: integerValue(
      readField(snapshot, "accrued_interest", "accruedInterest"),
      "positionSnapshot.accrued_interest"
    ),
    health_factor: integerValue(
      readField(snapshot, "health_factor", "healthFactor"),
      "positionSnapshot.health_factor"
    ),
    pool_config: poolConfig,
    collateral_token: readField(snapshot, "collateral_token", "collateralToken"),
    borrow_token: readField(snapshot, "borrow_token", "borrowToken"),
  };
}

export function normalizePosition(value) {
  const position = requireObject(value, "position");
  return {
    deposited_shares: integerValue(
      readField(position, "deposited_shares", "depositedShares"),
      "position.deposited_shares"
    ),
    borrowed_amount: integerValue(
      readField(position, "borrowed_amount", "borrowedAmount"),
      "position.borrowed_amount"
    ),
    last_update: integerValue(readField(position, "last_update", "lastUpdate"), "position.last_update"),
  };
}

export function normalizePoolConfig(value) {
  const poolConfig = requireObject(value, "poolConfig");
  return {
    max_ltv: integerValue(readField(poolConfig, "max_ltv", "maxLtv"), "poolConfig.max_ltv"),
    liq_threshold: integerValue(
      readField(poolConfig, "liq_threshold", "liqThreshold"),
      "poolConfig.liq_threshold"
    ),
    interest_rate: integerValue(
      readField(poolConfig, "interest_rate", "interestRate"),
      "poolConfig.interest_rate"
    ),
  };
}

export function priceTupleToNumber(value, decimals) {
  if (Array.isArray(value)) {
    return scaleIntegerToNumber(
      integerValue(value[0], "oracle.lastprice()[0]"),
      integerValue(decimals, "oracle.decimals()")
    );
  }

  const priceObject = requireObject(value, "oracle.lastprice()");
  return scaleIntegerToNumber(
    integerValue(priceObject.price, "oracle.lastprice().price"),
    integerValue(decimals, "oracle.decimals()")
  );
}

function readField(object, snakeName, camelName) {
  if (Object.hasOwn(object, snakeName)) {
    return object[snakeName];
  }
  if (Object.hasOwn(object, camelName)) {
    return object[camelName];
  }
  throw new TypeError(`${snakeName} or ${camelName} is required`);
}

function readAny(object, ...fieldNames) {
  for (const fieldName of fieldNames) {
    const value = object[fieldName];
    if (value !== undefined) {
      return value;
    }
  }
  throw new TypeError(`${fieldNames.join(" or ")} must be provided`);
}

function requiredMethod(object, ...methodNames) {
  const value = readAny(object, ...methodNames);
  if (typeof value === "function") {
    return value.bind(object);
  }
  throw new TypeError(`${methodNames.join(" or ")} must be a function`);
}

function integerValue(value, fieldName) {
  if (typeof value === "bigint") {
    return value.toString();
  }
  if (typeof value === "number" && Number.isInteger(value)) {
    return value.toString();
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    return value;
  }
  throw new TypeError(`${fieldName} must be an integer, integer string, or bigint`);
}

function numberValue(value, fieldName) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError(`${fieldName} must be a finite number`);
  }
  return value;
}

function stringValue(value, fieldName) {
  if (
    value !== null &&
    typeof value === "object" &&
    typeof value.toString === "function" &&
    value.toString !== Object.prototype.toString
  ) {
    value = value.toString();
  }
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`${fieldName} must be a non-empty string`);
  }
  return value;
}

function requireObject(value, fieldName) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${fieldName} must be an object`);
  }
  return value;
}
