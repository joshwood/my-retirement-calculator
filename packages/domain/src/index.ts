const BPS_DENOMINATOR = 10_000n;
const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;
const MAX_ACCOUNTS = 100;

export type AccountType =
  | "traditional_ira"
  | "roth_ira"
  | "brokerage"
  | "employer401k"
  | "cash"
  | "other";

export interface PlanProfile {
  currentAge: number;
  currentAnnualIncomeCents: bigint;
  projectionYears: number;
}

export interface Account {
  id: string;
  name: string;
  accountType: AccountType;
  otherTypeLabel: string | null;
  startingBalanceCents: bigint;
  costBasisCents: bigint;
  annualGrowthBps: number;
  annualDividendYieldBps: number;
  contributionAllocationBps: number;
  reinvestDividends: boolean;
}

export interface RetirementPlan {
  id: string;
  revision: bigint;
  profile: PlanProfile;
  accounts: Account[];
  createdAt: Date;
  updatedAt: Date;
}

export type ValidationCode =
  | "out_of_range"
  | "required"
  | "unexpected"
  | "duplicate"
  | "too_many"
  | "aggregate_out_of_range";

export interface ValidationError {
  path: string;
  code: ValidationCode;
  message: string;
}

export class ValidationErrors extends Error {
  constructor(readonly errors: readonly ValidationError[]) {
    super(`${String(errors.length)} validation error(s)`);
  }
}

export type CalculationCode = "arithmetic_overflow" | "zero_floor_invariant";

export class CalculationError extends Error {
  constructor(readonly code: CalculationCode) {
    super(code === "arithmetic_overflow" ? "calculation range exceeded" : "zero-floor invariant violated");
  }
}

export type ProjectedAccount = Account;

export interface AccountProjectionRow {
  accountId: string;
  openingBalanceCents: bigint;
  contributionCents: bigint;
  investedBalanceCents: bigint;
  appreciationCents: bigint;
  dividendGeneratedCents: bigint;
  dividendReinvestedCents: bigint;
  incomePaidCents: bigint;
  closingBalanceCents: bigint;
  cumulativeIncomePaidCents: bigint;
}

export type PortfolioProjectionRow = Omit<AccountProjectionRow, "accountId">;

export interface ProjectionYear {
  year: number;
  age: number;
  accounts: AccountProjectionRow[];
  portfolio: PortfolioProjectionRow;
}

export interface Projection {
  profile: PlanProfile;
  accounts: ProjectedAccount[];
  years: ProjectionYear[];
}

export function boundaryName(): "domain" {
  return "domain";
}

/** Applies basis points using exact integer math and midpoint-away-from-zero rounding. */
export function applyBps(valueCents: bigint, bps: number): bigint {
  assertI64(valueCents);
  if (!Number.isInteger(bps) || bps < -2_147_483_648 || bps > 2_147_483_647) overflow();
  const product = valueCents * BigInt(bps);
  const quotient = product / BPS_DENOMINATOR;
  const remainder = product % BPS_DENOMINATOR;
  const rounded = absolute(remainder) * 2n >= BPS_DENOMINATOR
    ? quotient + (product < 0n ? -1n : 1n)
    : quotient;
  return checkedI64(rounded);
}

/** Validates all save-time fields. Empty account drafts are intentionally valid. */
export function validateForSave(profile: PlanProfile, accounts: readonly Account[]): void {
  const errors: ValidationError[] = [];
  validateProfile(profile, errors);
  validateAccounts(accounts, errors);
  if (errors.length > 0) throw new ValidationErrors(errors);
}

/** Projects a complete draft, returning no object if validation or arithmetic fails. */
export function project(profile: PlanProfile, accounts: readonly Account[]): Projection {
  let errors: ValidationError[] = [];
  try {
    validateForSave(profile, accounts);
  } catch (error) {
    if (!(error instanceof ValidationErrors)) throw error;
    errors = [...error.errors];
  }
  if (accounts.length === 0) {
    errors.push(validationError("accounts", "required", "At least one account is required to project"));
  }
  if (errors.length > 0) throw new ValidationErrors(errors);

  const closingBalances = accounts.map((account) => account.startingBalanceCents);
  const cumulativeIncome = accounts.map(() => 0n);
  const years: ProjectionYear[] = [];

  for (let year = 1; year <= profile.projectionYears; year++) {
    const accountRows: AccountProjectionRow[] = [];
    let portfolio = emptyPortfolio();
    accounts.forEach((account, index) => {
      const opening = closingBalances[index];
      const priorIncome = cumulativeIncome[index];
      if (opening === undefined || priorIncome === undefined) overflow();
      const contribution = applyBps(profile.currentAnnualIncomeCents, account.contributionAllocationBps);
      const invested = checkedAdd(opening, contribution);
      const appreciation = applyBps(invested, account.annualGrowthBps);
      const dividendGenerated = applyBps(invested, account.annualDividendYieldBps);
      const nonDividendRemainder = checkedAdd(invested, appreciation);
      if (nonDividendRemainder < 0n) throw new CalculationError("zero_floor_invariant");
      const dividendReinvested = account.reinvestDividends ? dividendGenerated : 0n;
      const incomePaid = account.reinvestDividends ? 0n : dividendGenerated;
      const closing = checkedAdd(nonDividendRemainder, dividendReinvested);
      const newCumulativeIncome = checkedAdd(priorIncome, incomePaid);
      closingBalances[index] = closing;
      cumulativeIncome[index] = newCumulativeIncome;
      const row: AccountProjectionRow = {
        accountId: account.id,
        openingBalanceCents: opening,
        contributionCents: contribution,
        investedBalanceCents: invested,
        appreciationCents: appreciation,
        dividendGeneratedCents: dividendGenerated,
        dividendReinvestedCents: dividendReinvested,
        incomePaidCents: incomePaid,
        closingBalanceCents: closing,
        cumulativeIncomePaidCents: newCumulativeIncome,
      };
      portfolio = addToPortfolio(portfolio, row);
      accountRows.push(row);
    });
    years.push({ year, age: profile.currentAge + year, accounts: accountRows, portfolio });
  }

  return {
    profile: { ...profile },
    accounts: accounts.map((account) => ({ ...account })),
    years,
  };
}

/** Stored and stateless projections share this exact route-neutral model. */
export function projectRetirementPlan(plan: RetirementPlan): Projection {
  return project(plan.profile, plan.accounts);
}

function validateProfile(profile: PlanProfile, errors: ValidationError[]): void {
  if (!integerInRange(profile.currentAge, 18, 100)) {
    errors.push(validationError("profile.current_age", "out_of_range", "Must be between 18 and 100"));
  }
  if (profile.currentAnnualIncomeCents < 0n || profile.currentAnnualIncomeCents > 10_000_000_000n) {
    errors.push(validationError("profile.current_annual_income_cents", "out_of_range", "Must be between 0 and 10000000000"));
  }
  if (!integerInRange(profile.projectionYears, 1, 80)) {
    errors.push(validationError("profile.projection_years", "out_of_range", "Must be between 1 and 80"));
  }
  if (!Number.isSafeInteger(profile.currentAge + profile.projectionYears) || profile.currentAge + profile.projectionYears > 120) {
    errors.push(validationError("profile.projection_years", "out_of_range", "Current age plus projection years must not exceed 120"));
  }
}

function validateAccounts(accounts: readonly Account[], errors: ValidationError[]): void {
  if (accounts.length > MAX_ACCOUNTS) {
    errors.push(validationError("accounts", "too_many", "At most 100 accounts are allowed"));
  }
  const ids = new Set<string>();
  let allocationTotal = 0;
  accounts.forEach((account, index) => {
    const prefix = `accounts[${String(index)}]`;
    if (ids.has(account.id)) errors.push(validationError(`${prefix}.id`, "duplicate", "Account IDs must be unique within a plan"));
    ids.add(account.id);
    const nameLength = Array.from(account.name).length;
    if (account.name.trim() === "") errors.push(validationError(`${prefix}.name`, "required", "Must not be blank"));
    else if (nameLength > 80) errors.push(validationError(`${prefix}.name`, "out_of_range", "Must contain at most 80 Unicode scalar values"));

    if (account.accountType === "other") {
      if (account.otherTypeLabel === null || account.otherTypeLabel.trim() === "") {
        errors.push(validationError(`${prefix}.other_type_label`, "required", "A nonblank label is required for other accounts"));
      }
    } else if (account.otherTypeLabel !== null) {
      errors.push(validationError(`${prefix}.other_type_label`, "unexpected", "Only other accounts may have an other type label"));
    }
    bigintRange(account.startingBalanceCents, 0n, I64_MAX, `${prefix}.starting_balance_cents`, "Must be nonnegative", errors);
    bigintRange(account.costBasisCents, 0n, I64_MAX, `${prefix}.cost_basis_cents`, "Must be nonnegative", errors);
    numberRange(account.annualGrowthBps, -10_000, 100_000, `${prefix}.annual_growth_bps`, "Must be between -10000 and 100000", errors);
    numberRange(account.annualDividendYieldBps, 0, 10_000, `${prefix}.annual_dividend_yield_bps`, "Must be between 0 and 10000", errors);
    numberRange(account.contributionAllocationBps, 0, 10_000, `${prefix}.contribution_allocation_bps`, "Must be between 0 and 10000", errors);
    allocationTotal += account.contributionAllocationBps;
  });
  if (allocationTotal > 10_000) {
    errors.push(validationError("accounts.contribution_allocation_bps", "aggregate_out_of_range", "Aggregate contribution allocation must not exceed 10000"));
  }
}

function emptyPortfolio(): PortfolioProjectionRow {
  return {
    openingBalanceCents: 0n,
    contributionCents: 0n,
    investedBalanceCents: 0n,
    appreciationCents: 0n,
    dividendGeneratedCents: 0n,
    dividendReinvestedCents: 0n,
    incomePaidCents: 0n,
    closingBalanceCents: 0n,
    cumulativeIncomePaidCents: 0n,
  };
}

function addToPortfolio(total: PortfolioProjectionRow, row: AccountProjectionRow): PortfolioProjectionRow {
  return {
    openingBalanceCents: checkedAdd(total.openingBalanceCents, row.openingBalanceCents),
    contributionCents: checkedAdd(total.contributionCents, row.contributionCents),
    investedBalanceCents: checkedAdd(total.investedBalanceCents, row.investedBalanceCents),
    appreciationCents: checkedAdd(total.appreciationCents, row.appreciationCents),
    dividendGeneratedCents: checkedAdd(total.dividendGeneratedCents, row.dividendGeneratedCents),
    dividendReinvestedCents: checkedAdd(total.dividendReinvestedCents, row.dividendReinvestedCents),
    incomePaidCents: checkedAdd(total.incomePaidCents, row.incomePaidCents),
    closingBalanceCents: checkedAdd(total.closingBalanceCents, row.closingBalanceCents),
    cumulativeIncomePaidCents: checkedAdd(total.cumulativeIncomePaidCents, row.cumulativeIncomePaidCents),
  };
}

function checkedAdd(left: bigint, right: bigint): bigint {
  return checkedI64(left + right);
}

function assertI64(value: bigint): void {
  if (value < I64_MIN || value > I64_MAX) overflow();
}

function checkedI64(value: bigint): bigint {
  assertI64(value);
  return value;
}

function overflow(): never {
  throw new CalculationError("arithmetic_overflow");
}

function absolute(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function integerInRange(value: number, minimum: number, maximum: number): boolean {
  return Number.isInteger(value) && value >= minimum && value <= maximum;
}

function numberRange(value: number, minimum: number, maximum: number, path: string, message: string, errors: ValidationError[]): void {
  if (!integerInRange(value, minimum, maximum)) errors.push(validationError(path, "out_of_range", message));
}

function bigintRange(value: bigint, minimum: bigint, maximum: bigint, path: string, message: string, errors: ValidationError[]): void {
  if (value < minimum || value > maximum) errors.push(validationError(path, "out_of_range", message));
}

function validationError(path: string, code: ValidationCode, message: string): ValidationError {
  return { path, code, message };
}
