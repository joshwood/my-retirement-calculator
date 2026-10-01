export const CONTRACT_VERSION = "v1" as const;

export type AccountType =
  | "traditional_ira"
  | "roth_ira"
  | "brokerage"
  | "employer401k"
  | "cash"
  | "other";

export interface PlanProfile {
  current_age: number;
  current_annual_income_cents: bigint;
  projection_years: number;
}

export interface AccountInput {
  id: string;
  name: string;
  account_type: AccountType;
  other_type_label: string | null;
  starting_balance_cents: bigint;
  cost_basis_cents: bigint;
  annual_growth_bps: number;
  annual_dividend_yield_bps: number;
  contribution_allocation_bps: number;
  reinvest_dividends: boolean;
}

export interface PlanInput {
  profile: PlanProfile;
  accounts: AccountInput[];
}

export interface UpdatePlanInput extends PlanInput {
  expected_revision: bigint;
}

export interface StoredProjectionInput {
  readonly __storedProjectionInput?: never;
}

export interface PlanResponse extends PlanInput {
  contract_version: string;
  plan_id: string;
  revision: bigint;
  created_at: string;
  updated_at: string;
}

export interface PlanReference {
  plan_id: string;
  revision: bigint;
}

export interface AccountProjectionRow {
  account_id: string;
  opening_balance_cents: bigint;
  contribution_cents: bigint;
  invested_balance_cents: bigint;
  appreciation_cents: bigint;
  dividend_generated_cents: bigint;
  dividend_reinvested_cents: bigint;
  income_paid_cents: bigint;
  closing_balance_cents: bigint;
  cumulative_income_paid_cents: bigint;
}

export interface PortfolioProjectionRow {
  opening_balance_cents: bigint;
  contribution_cents: bigint;
  invested_balance_cents: bigint;
  appreciation_cents: bigint;
  dividend_generated_cents: bigint;
  dividend_reinvested_cents: bigint;
  income_paid_cents: bigint;
  closing_balance_cents: bigint;
  cumulative_income_paid_cents: bigint;
}

export interface ProjectionYear {
  year: number;
  age: number;
  accounts: AccountProjectionRow[];
  portfolio: PortfolioProjectionRow;
}

export interface CalculationWarning {
  code: string;
  message: string;
}

export interface ProjectionResponse extends PlanInput {
  contract_version: string;
  plan_ref: PlanReference | null;
  years: ProjectionYear[];
  warnings: CalculationWarning[];
}

export interface FieldError {
  path: string;
  code: string;
  message: string;
}

export interface ErrorResponse {
  code: string;
  message: string;
  field_errors: FieldError[];
  request_id: string;
}

export interface HealthResponse {
  status: string;
  build_version: string;
  instance_id: string;
}
