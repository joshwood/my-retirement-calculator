use domain::{Account, AccountId, AccountType, PlanProfile, project};
use uuid::Uuid;

#[test]
fn every_valid_growth_rate_produces_nonnegative_closing_balances() {
    for growth_bps in -10_000_i32..=100_000 {
        let profile = PlanProfile {
            current_age: 40,
            current_annual_income_cents: 10_001,
            projection_years: 2,
        };
        let account = Account {
            id: AccountId::new(Uuid::from_u128(1)),
            name: "Property account".to_owned(),
            account_type: AccountType::Brokerage,
            other_type_label: None,
            starting_balance_cents: 99_999,
            cost_basis_cents: 0,
            annual_growth_bps: growth_bps,
            annual_dividend_yield_bps: 10_000,
            contribution_allocation_bps: 10_000,
            reinvest_dividends: true,
        };
        let projection = project(&profile, &[account]).expect("bounded values project");
        assert!(projection.years.iter().all(|year| {
            let row = &year.accounts[0];
            row.closing_balance_cents >= 0
                && row.invested_balance_cents + row.appreciation_cents >= 0
        }));
    }
}
