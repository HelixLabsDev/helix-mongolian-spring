const BPS_DENOMINATOR = 10_000;

const STATUS_VALUES = new Set(["active", "liquidatable", "liquidated", "closed"]);

export function mapStellarVaultSnapshotToHelixPosition(snapshot) {
  const input = requireObject(snapshot, "snapshot");
  const position = requireObject(input.position, "position");
  const poolConfig = requireObject(input.poolConfig, "poolConfig");
  const collateral = requireObject(input.collateral, "collateral");
  const debt = requireObject(input.debt, "debt");
  const prices = requireObject(input.prices, "prices");

  const userAddress = requireString(input.userAddress, "userAddress");
  const vaultContractId = requireString(input.vaultContractId, "vaultContractId");
  const collateralToken = requireString(collateral.symbol, "collateral.symbol");
  const debtToken = requireString(debt.symbol, "debt.symbol");

  const collateralAmount = scaleIntegerToNumber(
    requiredIntegerLike(collateral.assets, "collateral.assets"),
    requiredIntegerLike(collateral.decimals, "collateral.decimals"),
    "collateral.assets"
  );
  const debtAmount = scaleIntegerToNumber(
    requiredIntegerLike(position.borrowed_amount, "position.borrowed_amount"),
    requiredIntegerLike(debt.decimals, "debt.decimals"),
    "position.borrowed_amount"
  );
  const accruedInterest = scaleIntegerToNumber(
    requiredIntegerLike(input.accruedInterest, "accruedInterest"),
    requiredIntegerLike(debt.decimals, "debt.decimals"),
    "accruedInterest"
  );

  const collateralValueUsd = collateralAmount * requiredNumber(prices.collateralUsd, "prices.collateralUsd");
  const debtValueUsd = debtAmount * requiredNumber(prices.debtUsd, "prices.debtUsd");
  const ltvCurrent = collateralValueUsd === 0 ? 0 : debtValueUsd / collateralValueUsd;
  const healthFactor = bpsToRatio(requiredIntegerLike(input.healthFactorBps, "healthFactorBps"));
  const status = resolveStatus(input.status, debtAmount, collateralAmount, healthFactor);

  return {
    chain: "stellar",
    user_id: userAddress,
    position_id: `stellar:${vaultContractId}:${userAddress}`,
    collateral_token: collateralToken,
    collateral_amount: collateralAmount,
    collateral_value_usd: collateralValueUsd,
    debt_token: debtToken,
    debt_amount: debtAmount,
    debt_value_usd: debtValueUsd,
    health_factor: healthFactor,
    ltv_current: ltvCurrent,
    ltv_max: bpsToRatio(requiredIntegerLike(poolConfig.max_ltv, "poolConfig.max_ltv")),
    liquidation_threshold: bpsToRatio(
      requiredIntegerLike(poolConfig.liq_threshold, "poolConfig.liq_threshold")
    ),
    borrow_rate: bpsToRatio(requiredIntegerLike(poolConfig.interest_rate, "poolConfig.interest_rate")),
    accrued_interest: accruedInterest,
    last_updated: normalizeTimestamp(input.asOf ?? position.last_update, "asOf"),
    status,
  };
}

export function bpsToRatio(value) {
  return Number(value) / BPS_DENOMINATOR;
}

export function scaleIntegerToNumber(value, decimals, fieldName = "value") {
  const raw = BigInt(value);
  const scaleDecimals = Number(decimals);
  if (!Number.isInteger(scaleDecimals) || scaleDecimals < 0) {
    throw new TypeError(`${fieldName} decimals must be a non-negative integer`);
  }

  const negative = raw < 0n;
  const magnitude = negative ? -raw : raw;
  const scale = 10n ** BigInt(scaleDecimals);
  const whole = magnitude / scale;
  const fraction = magnitude % scale;

  if (fraction === 0n) {
    return Number(`${negative ? "-" : ""}${whole.toString()}`);
  }

  const fractionText = fraction.toString().padStart(scaleDecimals, "0").replace(/0+$/, "");
  return Number(`${negative ? "-" : ""}${whole.toString()}.${fractionText}`);
}

function resolveStatus(explicitStatus, debtAmount, collateralAmount, healthFactor) {
  if (explicitStatus !== undefined) {
    const status = requireString(explicitStatus, "status");
    if (!STATUS_VALUES.has(status)) {
      throw new TypeError(`status must be one of: ${Array.from(STATUS_VALUES).join(", ")}`);
    }
    return status;
  }

  if (debtAmount === 0 && collateralAmount === 0) {
    return "closed";
  }
  if (debtAmount > 0 && healthFactor < 1) {
    return "liquidatable";
  }
  return "active";
}

function normalizeTimestamp(value, fieldName) {
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    const seconds = Number(requiredIntegerLike(value, fieldName));
    return new Date(seconds * 1000).toISOString();
  }

  if (typeof value === "string") {
    const parsed = Date.parse(value);
    if (Number.isNaN(parsed)) {
      throw new TypeError(`${fieldName} must be an ISO timestamp or Unix seconds`);
    }
    return new Date(parsed).toISOString();
  }

  const seconds = Number(requiredIntegerLike(value, fieldName));
  return new Date(seconds * 1000).toISOString();
}

function requiredIntegerLike(value, fieldName) {
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number" && Number.isInteger(value)) {
    return BigInt(value);
  }
  if (typeof value === "string" && /^-?\d+$/.test(value)) {
    return BigInt(value);
  }
  throw new TypeError(`${fieldName} must be an integer, integer string, or bigint`);
}

function requiredNumber(value, fieldName) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError(`${fieldName} must be a finite number`);
  }
  return value;
}

function requireObject(value, fieldName) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError(`${fieldName} must be an object`);
  }
  return value;
}

function requireString(value, fieldName) {
  if (typeof value !== "string" || value.length === 0) {
    throw new TypeError(`${fieldName} must be a non-empty string`);
  }
  return value;
}
