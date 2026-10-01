import { parseJsonWithBigInts } from "./json.js";
import type {
  AccountInput,
  AccountProjectionRow,
  AccountType,
  CalculationWarning,
  ErrorResponse,
  FieldError,
  HealthResponse,
  PlanInput,
  PlanProfile,
  PlanReference,
  PlanResponse,
  PortfolioProjectionRow,
  ProjectionResponse,
  ProjectionYear,
  StoredProjectionInput,
  UpdatePlanInput,
} from "./types.js";

const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;
const U64_MAX = (1n << 64n) - 1n;
const ACCOUNT_TYPES = new Set<AccountType>([
  "traditional_ira",
  "roth_ira",
  "brokerage",
  "employer401k",
  "cash",
  "other",
]);

export class ContractValidationError extends TypeError {
  constructor(readonly path: string, message: string) {
    super(`${path}: ${message}`);
  }
}

export function decodePlanInput(source: string): PlanInput {
  return planInput(parseJsonWithBigInts(source), "$", true);
}

export function decodeUpdatePlanInput(source: string): UpdatePlanInput {
  const value = object(parseJsonWithBigInts(source), "$", ["expected_revision", "profile", "accounts"]);
  return {
    expected_revision: u64(value.expected_revision, "$.expected_revision"),
    profile: profile(value.profile, "$.profile", true),
    accounts: array(value.accounts, "$.accounts", (entry, path) => account(entry, path, true)),
  };
}

export function decodeStoredProjectionInput(source: string): StoredProjectionInput {
  object(parseJsonWithBigInts(source), "$", []);
  return {};
}

export function decodePlanResponse(source: string): PlanResponse {
  const value = object(parseJsonWithBigInts(source), "$", undefined);
  return {
    contract_version: string(value.contract_version, "$.contract_version"),
    plan_id: string(value.plan_id, "$.plan_id"),
    revision: u64(value.revision, "$.revision"),
    created_at: string(value.created_at, "$.created_at"),
    updated_at: string(value.updated_at, "$.updated_at"),
    profile: profile(value.profile, "$.profile", false),
    accounts: array(value.accounts, "$.accounts", (entry, path) => account(entry, path, false)),
  };
}

export function decodeProjectionResponse(source: string): ProjectionResponse {
  const value = object(parseJsonWithBigInts(source), "$", undefined);
  return {
    contract_version: string(value.contract_version, "$.contract_version"),
    plan_ref: value.plan_ref === null ? null : planReference(value.plan_ref, "$.plan_ref"),
    profile: profile(value.profile, "$.profile", false),
    accounts: array(value.accounts, "$.accounts", (entry, path) => account(entry, path, false)),
    years: array(value.years, "$.years", projectionYear),
    warnings: array(value.warnings, "$.warnings", warning),
  };
}

export function decodeErrorResponse(source: string): ErrorResponse {
  const value = object(parseJsonWithBigInts(source), "$", undefined);
  return {
    code: string(value.code, "$.code"),
    message: string(value.message, "$.message"),
    field_errors: array(value.field_errors, "$.field_errors", fieldError),
    request_id: string(value.request_id, "$.request_id"),
  };
}

export function decodeHealthResponse(source: string): HealthResponse {
  const value = object(parseJsonWithBigInts(source), "$", undefined);
  return {
    status: string(value.status, "$.status"),
    build_version: string(value.build_version, "$.build_version"),
    instance_id: string(value.instance_id, "$.instance_id"),
  };
}

function planInput(input: unknown, path: string, exact: boolean): PlanInput {
  const value = object(input, path, exact ? ["profile", "accounts"] : undefined);
  return {
    profile: profile(value.profile, `${path}.profile`, exact),
    accounts: array(value.accounts, `${path}.accounts`, (entry, entryPath) => account(entry, entryPath, exact)),
  };
}

function profile(input: unknown, path: string, exact: boolean): PlanProfile {
  const value = object(
    input,
    path,
    exact ? ["current_age", "current_annual_income_cents", "projection_years"] : undefined,
  );
  return {
    current_age: boundedNumber(value.current_age, `${path}.current_age`, 0, 65_535),
    current_annual_income_cents: i64(value.current_annual_income_cents, `${path}.current_annual_income_cents`),
    projection_years: boundedNumber(value.projection_years, `${path}.projection_years`, 0, 65_535),
  };
}

function account(input: unknown, path: string, exact: boolean): AccountInput {
  const keys = [
    "id",
    "name",
    "account_type",
    "other_type_label",
    "starting_balance_cents",
    "cost_basis_cents",
    "annual_growth_bps",
    "annual_dividend_yield_bps",
    "contribution_allocation_bps",
    "reinvest_dividends",
  ];
  const value = object(input, path, exact ? keys : undefined);
  const accountType = string(value.account_type, `${path}.account_type`);
  if (!ACCOUNT_TYPES.has(accountType as AccountType)) fail(`${path}.account_type`, "unsupported account type");
  return {
    id: string(value.id, `${path}.id`),
    name: string(value.name, `${path}.name`),
    account_type: accountType as AccountType,
    other_type_label: nullableString(value.other_type_label, `${path}.other_type_label`),
    starting_balance_cents: i64(value.starting_balance_cents, `${path}.starting_balance_cents`),
    cost_basis_cents: i64(value.cost_basis_cents, `${path}.cost_basis_cents`),
    annual_growth_bps: boundedNumber(value.annual_growth_bps, `${path}.annual_growth_bps`, -2_147_483_648, 2_147_483_647),
    annual_dividend_yield_bps: boundedNumber(value.annual_dividend_yield_bps, `${path}.annual_dividend_yield_bps`, -2_147_483_648, 2_147_483_647),
    contribution_allocation_bps: boundedNumber(value.contribution_allocation_bps, `${path}.contribution_allocation_bps`, -2_147_483_648, 2_147_483_647),
    reinvest_dividends: boolean(value.reinvest_dividends, `${path}.reinvest_dividends`),
  };
}

function planReference(input: unknown, path: string): PlanReference {
  const value = object(input, path, undefined);
  return { plan_id: string(value.plan_id, `${path}.plan_id`), revision: u64(value.revision, `${path}.revision`) };
}

const MONEY_ROW_KEYS = [
  "opening_balance_cents",
  "contribution_cents",
  "invested_balance_cents",
  "appreciation_cents",
  "dividend_generated_cents",
  "dividend_reinvested_cents",
  "income_paid_cents",
  "closing_balance_cents",
  "cumulative_income_paid_cents",
] as const;

function moneyRow(input: unknown, path: string): PortfolioProjectionRow {
  const value = object(input, path, undefined);
  return Object.fromEntries(MONEY_ROW_KEYS.map((key) => [key, i64(value[key], `${path}.${key}`)])) as unknown as PortfolioProjectionRow;
}

function accountRow(input: unknown, path: string): AccountProjectionRow {
  const value = object(input, path, undefined);
  return { account_id: string(value.account_id, `${path}.account_id`), ...moneyRow(value, path) };
}

function projectionYear(input: unknown, path: string): ProjectionYear {
  const value = object(input, path, undefined);
  return {
    year: boundedNumber(value.year, `${path}.year`, 0, 65_535),
    age: boundedNumber(value.age, `${path}.age`, 0, 65_535),
    accounts: array(value.accounts, `${path}.accounts`, accountRow),
    portfolio: moneyRow(value.portfolio, `${path}.portfolio`),
  };
}

function warning(input: unknown, path: string): CalculationWarning {
  const value = object(input, path, undefined);
  return { code: string(value.code, `${path}.code`), message: string(value.message, `${path}.message`) };
}

function fieldError(input: unknown, path: string): FieldError {
  const value = object(input, path, undefined);
  return {
    path: string(value.path, `${path}.path`),
    code: string(value.code, `${path}.code`),
    message: string(value.message, `${path}.message`),
  };
}

function object(input: unknown, path: string, exactKeys: readonly string[] | undefined): Record<string, unknown> {
  if (input === null || typeof input !== "object" || Array.isArray(input)) fail(path, "expected object");
  const value = input as Record<string, unknown>;
  if (exactKeys !== undefined) {
    const allowed = new Set(exactKeys);
    const unexpected = Object.keys(value).find((key) => !allowed.has(key));
    if (unexpected !== undefined) fail(`${path}.${unexpected}`, "unknown field");
    const missing = exactKeys.find((key) => !(key in value));
    if (missing !== undefined) fail(`${path}.${missing}`, "missing field");
  }
  return value;
}

function array<T>(input: unknown, path: string, decode: (entry: unknown, path: string) => T): T[] {
  if (!Array.isArray(input)) fail(path, "expected array");
  return input.map((entry, index) => decode(entry, `${path}[${String(index)}]`));
}

function string(input: unknown, path: string): string {
  if (typeof input !== "string") fail(path, "expected string");
  return input;
}

function nullableString(input: unknown, path: string): string | null {
  return input === null ? null : string(input, path);
}

function boolean(input: unknown, path: string): boolean {
  if (typeof input !== "boolean") fail(path, "expected boolean");
  return input;
}

function i64(input: unknown, path: string): bigint {
  if (typeof input !== "bigint" || input < I64_MIN || input > I64_MAX) fail(path, "expected signed 64-bit JSON integer");
  return input;
}

function u64(input: unknown, path: string): bigint {
  if (typeof input !== "bigint" || input < 0n || input > U64_MAX) fail(path, "expected unsigned 64-bit JSON integer");
  return input;
}

function boundedNumber(input: unknown, path: string, minimum: number, maximum: number): number {
  if (typeof input !== "bigint" || input < BigInt(minimum) || input > BigInt(maximum)) fail(path, `expected integer from ${String(minimum)} through ${String(maximum)}`);
  return Number(input);
}

function fail(path: string, message: string): never {
  throw new ContractValidationError(path, message);
}
