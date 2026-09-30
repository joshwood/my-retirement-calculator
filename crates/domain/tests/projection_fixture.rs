use domain::{
    Account, AccountId, AccountType, CalculationError, PlanProfile, ProjectionError,
    ValidationCode, project, validate_for_save,
};
use uuid::Uuid;

fn account(id: u128) -> Account {
    Account {
        id: AccountId::new(Uuid::from_u128(id)),
        name: "Roth IRA".to_owned(),
        account_type: AccountType::RothIra,
        other_type_label: None,
        starting_balance_cents: 10_000_000,
        cost_basis_cents: 7_000_000,
        annual_growth_bps: 600,
        annual_dividend_yield_bps: 500,
        contribution_allocation_bps: 1_000,
        reinvest_dividends: true,
    }
}

fn profile(years: u16) -> PlanProfile {
    PlanProfile {
        current_age: 40,
        current_annual_income_cents: 10_000_000,
        projection_years: years,
    }
}

#[test]
fn annual_order_fixture_uses_independent_rounding_and_feeds_closing_forward() {
    let mut input = account(1);
    input.starting_balance_cents = 1;
    input.cost_basis_cents = 99;
    input.contribution_allocation_bps = 5_000;
    input.annual_growth_bps = 5_000;
    input.annual_dividend_yield_bps = 5_000;
    let input_profile = PlanProfile {
        current_age: 40,
        current_annual_income_cents: 1,
        projection_years: 2,
    };

    let output = project(&input_profile, &[input]).expect("fixture projects");
    let first = &output.years[0].accounts[0];
    assert_eq!(first.opening_balance_cents, 1);
    assert_eq!(first.contribution_cents, 1);
    assert_eq!(first.invested_balance_cents, 2);
    assert_eq!(first.appreciation_cents, 1);
    assert_eq!(first.dividend_generated_cents, 1);
    assert_eq!(first.closing_balance_cents, 4);
    assert_eq!(output.years[1].accounts[0].opening_balance_cents, 4);
    assert_eq!(output.years[0].year, 1);
    assert_eq!(output.years[0].age, 41);
    assert_eq!(output.accounts[0].cost_basis_cents, 99);
}

#[test]
fn negative_half_cent_appreciation_rounds_away_from_zero_independently() {
    let mut input = account(1);
    input.starting_balance_cents = 1;
    input.contribution_allocation_bps = 0;
    input.annual_growth_bps = -5_000;
    input.annual_dividend_yield_bps = 5_000;

    let output = project(&profile(1), &[input]).expect("fixture projects");
    let row = &output.years[0].accounts[0];
    assert_eq!(row.invested_balance_cents, 1);
    assert_eq!(row.appreciation_cents, -1);
    assert_eq!(row.dividend_generated_cents, 1);
    assert_eq!(row.closing_balance_cents, 1);
}

#[test]
fn contribution_and_portfolio_total_fixture_are_exact() {
    let mut second = account(2);
    second.name = "Brokerage".to_owned();
    second.account_type = AccountType::Brokerage;
    second.starting_balance_cents = 0;
    second.contribution_allocation_bps = 0;
    let output = project(&profile(1), &[account(1), second]).expect("fixture projects");
    let year = &output.years[0];
    assert_eq!(output.accounts[0].id, AccountId::new(Uuid::from_u128(1)));
    assert_eq!(output.accounts[1].id, AccountId::new(Uuid::from_u128(2)));
    assert_eq!(
        year.accounts[0].account_id,
        AccountId::new(Uuid::from_u128(1))
    );
    assert_eq!(
        year.accounts[1].account_id,
        AccountId::new(Uuid::from_u128(2))
    );
    assert_eq!(year.accounts[0].contribution_cents, 1_000_000);
    let first = &year.accounts[0];
    let second = &year.accounts[1];
    assert_eq!(
        year.portfolio.opening_balance_cents,
        first.opening_balance_cents + second.opening_balance_cents
    );
    assert_eq!(
        year.portfolio.contribution_cents,
        first.contribution_cents + second.contribution_cents
    );
    assert_eq!(
        year.portfolio.invested_balance_cents,
        first.invested_balance_cents + second.invested_balance_cents
    );
    assert_eq!(
        year.portfolio.appreciation_cents,
        first.appreciation_cents + second.appreciation_cents
    );
    assert_eq!(
        year.portfolio.dividend_generated_cents,
        first.dividend_generated_cents + second.dividend_generated_cents
    );
    assert_eq!(
        year.portfolio.dividend_reinvested_cents,
        first.dividend_reinvested_cents + second.dividend_reinvested_cents
    );
    assert_eq!(
        year.portfolio.income_paid_cents,
        first.income_paid_cents + second.income_paid_cents
    );
    assert_eq!(
        year.portfolio.closing_balance_cents,
        first.closing_balance_cents + second.closing_balance_cents
    );
    assert_eq!(
        year.portfolio.cumulative_income_paid_cents,
        first.cumulative_income_paid_cents + second.cumulative_income_paid_cents
    );
}

#[test]
fn dividend_policy_fixture_reinvests_or_pays_exactly() {
    let mut off = account(1);
    off.starting_balance_cents = 10_000_000;
    off.contribution_allocation_bps = 0;
    off.annual_growth_bps = 0;
    off.annual_dividend_yield_bps = 500;
    off.reinvest_dividends = false;
    let mut on = off.clone();
    on.id = AccountId::new(Uuid::from_u128(2));
    on.reinvest_dividends = true;

    let off_output = project(&profile(2), &[off]).expect("payout projects");
    let off_first = &off_output.years[0].accounts[0];
    assert_eq!(off_first.dividend_generated_cents, 500_000);
    assert_eq!(off_first.income_paid_cents, 500_000);
    assert_eq!(off_first.dividend_reinvested_cents, 0);
    assert_eq!(off_first.closing_balance_cents, 10_000_000);
    assert_eq!(
        off_output.years[1].accounts[0].cumulative_income_paid_cents,
        1_000_000
    );

    let on_output = project(&profile(1), &[on]).expect("reinvestment projects");
    let on_first = &on_output.years[0].accounts[0];
    assert_eq!(on_first.income_paid_cents, 0);
    assert_eq!(on_first.dividend_reinvested_cents, 500_000);
    assert_eq!(on_first.closing_balance_cents, 10_500_000);
}

#[test]
fn exactly_negative_one_hundred_percent_leaves_only_reinvested_dividend() {
    let mut input = account(1);
    input.contribution_allocation_bps = 0;
    input.annual_growth_bps = -10_000;
    input.annual_dividend_yield_bps = 500;
    let output = project(&profile(1), &[input]).expect("valid floor projects");
    let row = &output.years[0].accounts[0];
    assert_eq!(row.invested_balance_cents + row.appreciation_cents, 0);
    assert_eq!(row.closing_balance_cents, row.dividend_reinvested_cents);
}

#[test]
fn overflow_returns_no_partial_projection() {
    let mut input = account(1);
    input.starting_balance_cents = i64::MAX;
    input.contribution_allocation_bps = 1;
    input.annual_growth_bps = 0;
    input.annual_dividend_yield_bps = 0;
    assert_eq!(
        project(&profile(1), &[input]),
        Err(ProjectionError::Calculation(
            CalculationError::ArithmeticOverflow
        ))
    );
}

#[test]
fn checked_portfolio_totals_and_cumulative_income_reject_overflow() {
    let mut first = account(1);
    first.starting_balance_cents = i64::MAX / 2 + 1;
    first.contribution_allocation_bps = 0;
    first.annual_growth_bps = 0;
    first.annual_dividend_yield_bps = 0;
    let mut second = first.clone();
    second.id = AccountId::new(Uuid::from_u128(2));
    assert_eq!(
        project(&profile(1), &[first, second]),
        Err(ProjectionError::Calculation(
            CalculationError::ArithmeticOverflow
        ))
    );

    let mut payout = account(3);
    payout.starting_balance_cents = i64::MAX / 2 + 1;
    payout.contribution_allocation_bps = 0;
    payout.annual_growth_bps = 0;
    payout.annual_dividend_yield_bps = 10_000;
    payout.reinvest_dividends = false;
    assert_eq!(
        project(&profile(2), &[payout]),
        Err(ProjectionError::Calculation(
            CalculationError::ArithmeticOverflow
        ))
    );
}

#[test]
fn repeated_route_neutral_calculation_models_are_identical() {
    let accounts = vec![account(1)];
    let first = project(&profile(2), &accounts).expect("first projection");
    let second = project(&profile(2), &accounts).expect("second projection");
    assert_eq!(first, second);
}

#[test]
fn empty_drafts_save_but_do_not_project() {
    assert_eq!(validate_for_save(&profile(1), &[]), Ok(()));
    let error = project(&profile(1), &[]).expect_err("empty projection is rejected");
    let ProjectionError::Validation(errors) = error else {
        panic!("expected validation error");
    };
    assert!(
        errors
            .as_slice()
            .iter()
            .any(|error| { error.path == "accounts" && error.code == ValidationCode::Required })
    );
}
