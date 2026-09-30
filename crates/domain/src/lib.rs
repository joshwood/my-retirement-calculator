//! Pure retirement-plan entities, validation, and deterministic projections.

use std::collections::HashSet;
use std::fmt;
use std::time::SystemTime;

use uuid::Uuid;

const BPS_DENOMINATOR: i128 = 10_000;
const MAX_ACCOUNTS: usize = 100;

/// Identifies the domain boundary for composition-root dependency checks.
#[must_use]
pub const fn boundary_name() -> &'static str {
    "domain"
}

/// Stable identity of a saved retirement plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlanId(Uuid);

impl PlanId {
    #[must_use]
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> Uuid {
        self.0
    }
}

/// Stable identity of an account within a plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AccountId(Uuid);

impl AccountId {
    #[must_use]
    pub const fn new(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> Uuid {
        self.0
    }
}

/// A stored plan snapshot. Account order is significant and is preserved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetirementPlan {
    pub id: PlanId,
    pub revision: u64,
    pub profile: PlanProfile,
    pub accounts: Vec<Account>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl RetirementPlan {
    /// Validates a snapshot for persistence. Empty account drafts are allowed.
    ///
    /// # Errors
    ///
    /// Returns every field-addressable validation failure found in the snapshot.
    pub fn validate_for_save(&self) -> Result<(), ValidationErrors> {
        validate_for_save(&self.profile, &self.accounts)
    }

    /// Projects this exact snapshot without mutating it.
    ///
    /// # Errors
    ///
    /// Returns validation or checked-arithmetic errors without a partial result.
    pub fn project(&self) -> Result<Projection, ProjectionError> {
        project(&self.profile, &self.accounts)
    }
}

/// Plan-wide inputs used by every account projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanProfile {
    pub current_age: u16,
    pub current_annual_income_cents: i64,
    pub projection_years: u16,
}

/// Supported MVP account categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountType {
    TraditionalIra,
    RothIra,
    Brokerage,
    Employer401k,
    Cash,
    Other,
}

/// Inputs and metadata for one projected account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Account {
    pub id: AccountId,
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

/// Stable machine-readable category for a domain validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationCode {
    OutOfRange,
    Required,
    Unexpected,
    Duplicate,
    TooMany,
    AggregateOutOfRange,
}

/// A field-addressable validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub path: String,
    pub code: ValidationCode,
    pub message: &'static str,
}

/// All validation failures found in one pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    #[must_use]
    pub fn as_slice(&self) -> &[ValidationError] {
        &self.0
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<ValidationError> {
        self.0
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} validation error(s)", self.0.len())
    }
}

impl std::error::Error for ValidationErrors {}

/// Arithmetic failures return no partial projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalculationError {
    ArithmeticOverflow,
    ZeroFloorInvariant,
}

impl fmt::Display for CalculationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArithmeticOverflow => formatter.write_str("calculation range exceeded"),
            Self::ZeroFloorInvariant => formatter.write_str("zero-floor invariant violated"),
        }
    }
}

impl std::error::Error for CalculationError {}

/// Projection can fail before calculation (invalid inputs) or during checked arithmetic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionError {
    Validation(ValidationErrors),
    Calculation(CalculationError),
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => error.fmt(formatter),
            Self::Calculation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ProjectionError {}

impl From<ValidationErrors> for ProjectionError {
    fn from(value: ValidationErrors) -> Self {
        Self::Validation(value)
    }
}

impl From<CalculationError> for ProjectionError {
    fn from(value: CalculationError) -> Self {
        Self::Calculation(value)
    }
}

/// Route-neutral deterministic calculation output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Projection {
    pub profile: PlanProfile,
    pub accounts: Vec<ProjectedAccount>,
    pub years: Vec<ProjectionYear>,
}

/// Echoed account assumptions. Cost basis is metadata and is never projected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedAccount {
    pub id: AccountId,
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

impl From<&Account> for ProjectedAccount {
    fn from(account: &Account) -> Self {
        Self {
            id: account.id,
            name: account.name.clone(),
            account_type: account.account_type,
            other_type_label: account.other_type_label.clone(),
            starting_balance_cents: account.starting_balance_cents,
            cost_basis_cents: account.cost_basis_cents,
            annual_growth_bps: account.annual_growth_bps,
            annual_dividend_yield_bps: account.annual_dividend_yield_bps,
            contribution_allocation_bps: account.contribution_allocation_bps,
            reinvest_dividends: account.reinvest_dividends,
        }
    }
}

/// One annual portfolio snapshot with account rows in input order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionYear {
    pub year: u16,
    pub age: u16,
    pub accounts: Vec<AccountProjectionRow>,
    pub portfolio: PortfolioProjectionRow,
}

/// One account's checked annual calculation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountProjectionRow {
    pub account_id: AccountId,
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

/// Checked sums of all account rows for one year.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

/// Applies basis points with midpoint rounding away from zero.
///
/// Multiplication, rounding, and conversion are all checked.
///
/// # Errors
///
/// Returns [`CalculationError::ArithmeticOverflow`] if the rounded result cannot
/// be represented as an `i64`.
pub fn apply_bps(value_cents: i64, bps: i32) -> Result<i64, CalculationError> {
    let product = i128::from(value_cents)
        .checked_mul(i128::from(bps))
        .ok_or(CalculationError::ArithmeticOverflow)?;
    let quotient = product
        .checked_div(BPS_DENOMINATOR)
        .ok_or(CalculationError::ArithmeticOverflow)?;
    let remainder = product
        .checked_rem(BPS_DENOMINATOR)
        .ok_or(CalculationError::ArithmeticOverflow)?;
    let rounded = if remainder.abs() * 2 >= BPS_DENOMINATOR {
        quotient
            .checked_add(product.signum())
            .ok_or(CalculationError::ArithmeticOverflow)?
    } else {
        quotient
    };
    i64::try_from(rounded).map_err(|_| CalculationError::ArithmeticOverflow)
}

/// Validates values that may be persisted as an empty or populated draft.
///
/// # Errors
///
/// Returns every field-addressable validation failure found in the inputs.
pub fn validate_for_save(
    profile: &PlanProfile,
    accounts: &[Account],
) -> Result<(), ValidationErrors> {
    let mut errors = Vec::new();
    validate_profile(profile, &mut errors);
    validate_accounts(accounts, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ValidationErrors(errors))
    }
}

/// Produces a deterministic projection. At least one account is required.
///
/// # Errors
///
/// Returns validation or checked-arithmetic errors without a partial result.
pub fn project(profile: &PlanProfile, accounts: &[Account]) -> Result<Projection, ProjectionError> {
    let mut validation_errors = match validate_for_save(profile, accounts) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.into_vec(),
    };
    if accounts.is_empty() {
        validation_errors.push(validation_error(
            "accounts",
            ValidationCode::Required,
            "At least one account is required to project",
        ));
    }
    if !validation_errors.is_empty() {
        return Err(ValidationErrors(validation_errors).into());
    }

    let mut closing_balances = accounts
        .iter()
        .map(|account| account.starting_balance_cents)
        .collect::<Vec<_>>();
    let mut cumulative_income = vec![0_i64; accounts.len()];
    let mut years = Vec::with_capacity(usize::from(profile.projection_years));

    for year in 1..=profile.projection_years {
        let mut account_rows = Vec::with_capacity(accounts.len());
        let mut portfolio = PortfolioProjectionRow::default();
        for (index, account) in accounts.iter().enumerate() {
            let opening = closing_balances[index];
            let contribution = apply_bps(
                profile.current_annual_income_cents,
                account.contribution_allocation_bps,
            )?;
            let invested = checked_add(opening, contribution)?;
            let appreciation = apply_bps(invested, account.annual_growth_bps)?;
            let dividend_generated = apply_bps(invested, account.annual_dividend_yield_bps)?;
            let non_dividend_remainder = checked_add(invested, appreciation)?;
            if non_dividend_remainder < 0 {
                return Err(CalculationError::ZeroFloorInvariant.into());
            }
            let (dividend_reinvested, income_paid) = if account.reinvest_dividends {
                (dividend_generated, 0)
            } else {
                (0, dividend_generated)
            };
            let closing = checked_add(non_dividend_remainder, dividend_reinvested)?;
            cumulative_income[index] = checked_add(cumulative_income[index], income_paid)?;
            closing_balances[index] = closing;

            let row = AccountProjectionRow {
                account_id: account.id,
                opening_balance_cents: opening,
                contribution_cents: contribution,
                invested_balance_cents: invested,
                appreciation_cents: appreciation,
                dividend_generated_cents: dividend_generated,
                dividend_reinvested_cents: dividend_reinvested,
                income_paid_cents: income_paid,
                closing_balance_cents: closing,
                cumulative_income_paid_cents: cumulative_income[index],
            };
            portfolio.add_account(&row)?;
            account_rows.push(row);
        }
        years.push(ProjectionYear {
            year,
            age: profile
                .current_age
                .checked_add(year)
                .ok_or(CalculationError::ArithmeticOverflow)?,
            accounts: account_rows,
            portfolio,
        });
    }

    Ok(Projection {
        profile: profile.clone(),
        accounts: accounts.iter().map(ProjectedAccount::from).collect(),
        years,
    })
}

impl PortfolioProjectionRow {
    fn add_account(&mut self, row: &AccountProjectionRow) -> Result<(), CalculationError> {
        self.opening_balance_cents =
            checked_add(self.opening_balance_cents, row.opening_balance_cents)?;
        self.contribution_cents = checked_add(self.contribution_cents, row.contribution_cents)?;
        self.invested_balance_cents =
            checked_add(self.invested_balance_cents, row.invested_balance_cents)?;
        self.appreciation_cents = checked_add(self.appreciation_cents, row.appreciation_cents)?;
        self.dividend_generated_cents =
            checked_add(self.dividend_generated_cents, row.dividend_generated_cents)?;
        self.dividend_reinvested_cents = checked_add(
            self.dividend_reinvested_cents,
            row.dividend_reinvested_cents,
        )?;
        self.income_paid_cents = checked_add(self.income_paid_cents, row.income_paid_cents)?;
        self.closing_balance_cents =
            checked_add(self.closing_balance_cents, row.closing_balance_cents)?;
        self.cumulative_income_paid_cents = checked_add(
            self.cumulative_income_paid_cents,
            row.cumulative_income_paid_cents,
        )?;
        Ok(())
    }
}

fn checked_add(left: i64, right: i64) -> Result<i64, CalculationError> {
    left.checked_add(right)
        .ok_or(CalculationError::ArithmeticOverflow)
}

fn validate_profile(profile: &PlanProfile, errors: &mut Vec<ValidationError>) {
    if !(18..=100).contains(&profile.current_age) {
        errors.push(validation_error(
            "profile.current_age",
            ValidationCode::OutOfRange,
            "Must be between 18 and 100",
        ));
    }
    if !(0..=10_000_000_000).contains(&profile.current_annual_income_cents) {
        errors.push(validation_error(
            "profile.current_annual_income_cents",
            ValidationCode::OutOfRange,
            "Must be between 0 and 10000000000",
        ));
    }
    if !(1..=80).contains(&profile.projection_years) {
        errors.push(validation_error(
            "profile.projection_years",
            ValidationCode::OutOfRange,
            "Must be between 1 and 80",
        ));
    }
    if profile
        .current_age
        .checked_add(profile.projection_years)
        .is_none_or(|final_age| final_age > 120)
    {
        errors.push(validation_error(
            "profile.projection_years",
            ValidationCode::OutOfRange,
            "Current age plus projection years must not exceed 120",
        ));
    }
}

fn validate_accounts(accounts: &[Account], errors: &mut Vec<ValidationError>) {
    if accounts.len() > MAX_ACCOUNTS {
        errors.push(validation_error(
            "accounts",
            ValidationCode::TooMany,
            "At most 100 accounts are allowed",
        ));
    }

    let mut ids = HashSet::with_capacity(accounts.len());
    let mut allocation_total = 0_i64;
    for (index, account) in accounts.iter().enumerate() {
        validate_account(index, account, &mut ids, &mut allocation_total, errors);
    }
    if allocation_total > 10_000 {
        errors.push(validation_error(
            "accounts.contribution_allocation_bps",
            ValidationCode::AggregateOutOfRange,
            "Aggregate contribution allocation must not exceed 10000",
        ));
    }
}

fn validate_account(
    index: usize,
    account: &Account,
    ids: &mut HashSet<AccountId>,
    allocation_total: &mut i64,
    errors: &mut Vec<ValidationError>,
) {
    let prefix = format!("accounts[{index}]");
    if !ids.insert(account.id) {
        errors.push(validation_error(
            format!("{prefix}.id"),
            ValidationCode::Duplicate,
            "Account IDs must be unique within a plan",
        ));
    }
    let name_length = account.name.chars().count();
    if account.name.trim().is_empty() {
        errors.push(validation_error(
            format!("{prefix}.name"),
            ValidationCode::Required,
            "Must not be blank",
        ));
    } else if name_length > 80 {
        errors.push(validation_error(
            format!("{prefix}.name"),
            ValidationCode::OutOfRange,
            "Must contain at most 80 Unicode scalar values",
        ));
    }

    match account.account_type {
        AccountType::Other => {
            if account
                .other_type_label
                .as_deref()
                .is_none_or(|label| label.trim().is_empty())
            {
                errors.push(validation_error(
                    format!("{prefix}.other_type_label"),
                    ValidationCode::Required,
                    "A nonblank label is required for other accounts",
                ));
            }
        }
        _ if account.other_type_label.is_some() => errors.push(validation_error(
            format!("{prefix}.other_type_label"),
            ValidationCode::Unexpected,
            "Only other accounts may have an other type label",
        )),
        _ => {}
    }

    validate_range(
        &account.starting_balance_cents,
        &0,
        &i64::MAX,
        format!("{prefix}.starting_balance_cents"),
        "Must be nonnegative",
        errors,
    );
    validate_range(
        &account.cost_basis_cents,
        &0,
        &i64::MAX,
        format!("{prefix}.cost_basis_cents"),
        "Must be nonnegative",
        errors,
    );
    validate_range(
        &account.annual_growth_bps,
        &-10_000,
        &100_000,
        format!("{prefix}.annual_growth_bps"),
        "Must be between -10000 and 100000",
        errors,
    );
    validate_range(
        &account.annual_dividend_yield_bps,
        &0,
        &10_000,
        format!("{prefix}.annual_dividend_yield_bps"),
        "Must be between 0 and 10000",
        errors,
    );
    validate_range(
        &account.contribution_allocation_bps,
        &0,
        &10_000,
        format!("{prefix}.contribution_allocation_bps"),
        "Must be between 0 and 10000",
        errors,
    );
    *allocation_total += i64::from(account.contribution_allocation_bps);
}

fn validate_range<T: PartialOrd>(
    value: &T,
    minimum: &T,
    maximum: &T,
    path: String,
    message: &'static str,
    errors: &mut Vec<ValidationError>,
) {
    if value < minimum || value > maximum {
        errors.push(validation_error(path, ValidationCode::OutOfRange, message));
    }
}

fn validation_error(
    path: impl Into<String>,
    code: ValidationCode,
    message: &'static str,
) -> ValidationError {
    ValidationError {
        path: path.into(),
        code,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::{CalculationError, apply_bps};

    #[test]
    fn apply_bps_rounds_positive_and_negative_half_cents_away_from_zero() {
        assert_eq!(apply_bps(1, 5_000), Ok(1));
        assert_eq!(apply_bps(-1, 5_000), Ok(-1));
        assert_eq!(apply_bps(1, -5_000), Ok(-1));
        assert_eq!(apply_bps(-1, -5_000), Ok(1));
    }

    #[test]
    fn apply_bps_checks_i64_conversion() {
        assert_eq!(
            apply_bps(i64::MAX, 100_000),
            Err(CalculationError::ArithmeticOverflow)
        );
    }
}
