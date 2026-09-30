//! Versioned HTTP boundary data transfer objects.

use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: &str = "v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    pub profile: PlanProfile,
    pub accounts: Vec<AccountInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdatePlanInput {
    pub expected_revision: u64,
    pub profile: PlanProfile,
    pub accounts: Vec<AccountInput>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoredProjectionInput {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanProfile {
    pub current_age: u16,
    pub current_annual_income_cents: i64,
    pub projection_years: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountInput {
    pub id: String,
    pub name: String,
    pub account_type: AccountType,
    pub other_type_label: Option<String>,
    pub starting_balance_cents: i64,
    pub cost_basis_cents: i64,
    pub annual_growth_bps: i32,
    pub annual_dividend_yield_bps: i32,
    pub contribution_allocation_bps: i32,
    pub reinvest_dividends: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    TraditionalIra,
    RothIra,
    Brokerage,
    Employer401k,
    Cash,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanResponse {
    pub contract_version: String,
    pub plan_id: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
    pub profile: PlanProfile,
    pub accounts: Vec<AccountInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlanReference {
    pub plan_id: String,
    pub revision: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectionResponse {
    pub contract_version: String,
    pub plan_ref: Option<PlanReference>,
    pub profile: PlanProfile,
    pub accounts: Vec<AccountInput>,
    pub years: Vec<ProjectionYear>,
    pub warnings: Vec<CalculationWarning>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectionYear {
    pub year: u16,
    pub age: u16,
    pub accounts: Vec<AccountProjectionRow>,
    pub portfolio: PortfolioProjectionRow,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AccountProjectionRow {
    pub account_id: String,
    pub opening_balance_cents: i64,
    pub contribution_cents: i64,
    pub invested_balance_cents: i64,
    pub appreciation_cents: i64,
    pub dividend_generated_cents: i64,
    pub dividend_reinvested_cents: i64,
    pub income_paid_cents: i64,
    pub closing_balance_cents: i64,
    pub cumulative_income_paid_cents: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PortfolioProjectionRow {
    pub opening_balance_cents: i64,
    pub contribution_cents: i64,
    pub invested_balance_cents: i64,
    pub appreciation_cents: i64,
    pub dividend_generated_cents: i64,
    pub dividend_reinvested_cents: i64,
    pub income_paid_cents: i64,
    pub closing_balance_cents: i64,
    pub cumulative_income_paid_cents: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalculationWarning {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub field_errors: Vec<FieldError>,
    pub request_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FieldError {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HealthResponse {
    pub status: String,
    pub build_version: String,
    pub instance_id: String,
}
