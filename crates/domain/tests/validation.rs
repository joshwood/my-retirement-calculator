use domain::{Account, AccountId, AccountType, PlanProfile, ValidationCode, validate_for_save};
use uuid::Uuid;

fn valid_account(id: u128) -> Account {
    Account {
        id: AccountId::new(Uuid::from_u128(id)),
        name: "Account".to_owned(),
        account_type: AccountType::TraditionalIra,
        other_type_label: None,
        starting_balance_cents: 0,
        cost_basis_cents: 1,
        annual_growth_bps: -10_000,
        annual_dividend_yield_bps: 10_000,
        contribution_allocation_bps: 0,
        reinvest_dividends: false,
    }
}

fn valid_profile() -> PlanProfile {
    PlanProfile {
        current_age: 40,
        current_annual_income_cents: 10_000_000,
        projection_years: 20,
    }
}

#[test]
fn validates_all_profile_boundaries() {
    for profile in [
        PlanProfile {
            current_age: 17,
            ..valid_profile()
        },
        PlanProfile {
            current_age: 101,
            ..valid_profile()
        },
        PlanProfile {
            current_annual_income_cents: -1,
            ..valid_profile()
        },
        PlanProfile {
            current_annual_income_cents: 10_000_000_001,
            ..valid_profile()
        },
        PlanProfile {
            projection_years: 0,
            ..valid_profile()
        },
        PlanProfile {
            current_age: 100,
            projection_years: 21,
            ..valid_profile()
        },
    ] {
        assert!(validate_for_save(&profile, &[]).is_err());
    }
    assert!(
        validate_for_save(
            &PlanProfile {
                current_age: 40,
                current_annual_income_cents: 10_000_000_000,
                projection_years: 80,
            },
            &[]
        )
        .is_ok()
    );
}

#[test]
fn validates_account_fields_and_other_label_rule() {
    let mut invalid = valid_account(1);
    invalid.name = " ".to_owned();
    invalid.account_type = AccountType::Other;
    invalid.other_type_label = Some("\t".to_owned());
    invalid.starting_balance_cents = -1;
    invalid.cost_basis_cents = -1;
    invalid.annual_growth_bps = -10_001;
    invalid.annual_dividend_yield_bps = 10_001;
    invalid.contribution_allocation_bps = -1;
    let errors = validate_for_save(&valid_profile(), &[invalid])
        .expect_err("invalid account")
        .into_vec();
    for suffix in [
        "name",
        "other_type_label",
        "starting_balance_cents",
        "cost_basis_cents",
        "annual_growth_bps",
        "annual_dividend_yield_bps",
        "contribution_allocation_bps",
    ] {
        assert!(
            errors
                .iter()
                .any(|error| error.path == format!("accounts[0].{suffix}"))
        );
    }

    let mut unexpected = valid_account(1);
    unexpected.other_type_label = Some("Not allowed".to_owned());
    assert!(validate_for_save(&valid_profile(), &[unexpected]).is_err());
}

#[test]
fn name_limit_counts_unicode_scalar_values() {
    let mut account = valid_account(1);
    account.name = "🦀".repeat(80);
    assert!(validate_for_save(&valid_profile(), &[account.clone()]).is_ok());
    account.name.push('x');
    assert!(validate_for_save(&valid_profile(), &[account]).is_err());
}

#[test]
fn validates_unique_ids_account_cap_and_aggregate_allocation() {
    let duplicate = valid_account(1);
    let mut second = duplicate.clone();
    second.contribution_allocation_bps = 10_000;
    let mut first = duplicate;
    first.contribution_allocation_bps = 1;
    let errors = validate_for_save(&valid_profile(), &[first, second])
        .expect_err("duplicate and aggregate invalid")
        .into_vec();
    assert!(errors.iter().any(|error| {
        error.path == "accounts[1].id" && error.code == ValidationCode::Duplicate
    }));
    assert!(
        errors
            .iter()
            .any(|error| error.code == ValidationCode::AggregateOutOfRange)
    );

    let accounts = (0_u128..=100).map(valid_account).collect::<Vec<_>>();
    let errors = validate_for_save(&valid_profile(), &accounts)
        .expect_err("account cap")
        .into_vec();
    assert!(
        errors
            .iter()
            .any(|error| error.code == ValidationCode::TooMany)
    );
}

#[test]
fn accepts_every_supported_account_type_and_cost_basis_above_balance() {
    let types = [
        AccountType::TraditionalIra,
        AccountType::RothIra,
        AccountType::Brokerage,
        AccountType::Employer401k,
        AccountType::Cash,
        AccountType::Other,
    ];
    let accounts = types
        .into_iter()
        .enumerate()
        .map(|(index, account_type)| {
            let mut account = valid_account(index as u128);
            account.account_type = account_type;
            account.other_type_label =
                (account_type == AccountType::Other).then(|| "Pension".to_owned());
            account
        })
        .collect::<Vec<_>>();
    assert!(validate_for_save(&valid_profile(), &accounts).is_ok());
}
