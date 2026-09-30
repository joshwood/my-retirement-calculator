//! Leptos CSR retirement-planner workflow backed only by the shared v1 DTOs.

use api_contract::HealthResponse;

/// Decode a health response through the shared browser/server contract.
///
/// # Errors
///
/// Returns a JSON decoding error when the response does not match the v1 DTO.
pub fn decode_health(json: &str) -> Result<HealthResponse, serde_json::Error> {
    serde_json::from_str(json)
}

/// Format cents for display without changing editable input values.
#[must_use]
pub fn format_money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let absolute = i128::from(cents).abs();
    let dollars = (absolute / 100).to_string();
    let fraction = absolute % 100;
    let mut grouped = String::new();
    for (index, character) in dollars.chars().enumerate() {
        if index > 0 && (dollars.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    format!("{sign}${grouped}.{fraction:02}")
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_hundredths(value: &str) -> Result<i128, ()> {
    let value = value.trim();
    let (negative, unsigned) = if let Some(unsigned) = value.strip_prefix('-') {
        (true, unsigned)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if (whole.is_empty() && fraction.is_empty())
        || unsigned.matches('.').count() > 1
        || fraction.len() > 2
        || !whole.chars().all(|character| character.is_ascii_digit())
        || !fraction.chars().all(|character| character.is_ascii_digit())
    {
        return Err(());
    }
    let whole = if whole.is_empty() {
        0
    } else {
        whole.parse::<i128>().map_err(|_| ())?
    };
    let fraction = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i128>().map_err(|_| ())? * 10,
        2 => fraction.parse::<i128>().map_err(|_| ())?,
        _ => return Err(()),
    };
    let scaled = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or(())?;
    if negative {
        scaled.checked_neg().ok_or(())
    } else {
        Ok(scaled)
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::sync::atomic::{AtomicU64, Ordering};

    use api_contract::{
        AccountInput, AccountType, ErrorResponse, FieldError, HealthResponse, PlanInput,
        PlanProfile, PlanResponse, ProjectionResponse, UpdatePlanInput,
    };
    use leptos::{ev, prelude::*};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{HtmlInputElement, HtmlSelectElement, Request, RequestInit, Response};

    use super::format_money;

    const INSTANCE_KEY: &str = "retirement-planner.instance-id";
    static ACCOUNT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, PartialEq, Eq)]
    struct ProfileDraft {
        current_age: String,
        annual_income: String,
        projection_years: String,
    }
    impl Default for ProfileDraft {
        fn default() -> Self {
            Self {
                current_age: "40".into(),
                annual_income: "100000".into(),
                projection_years: "25".into(),
            }
        }
    }

    #[derive(Clone, PartialEq, Eq)]
    struct AccountDraft {
        id: String,
        name: String,
        account_type: AccountType,
        other_type_label: String,
        starting_balance: String,
        cost_basis: String,
        annual_growth_rate: String,
        dividend_yield: String,
        contribution_allocation: String,
        reinvest_dividends: bool,
    }
    impl AccountDraft {
        fn blank() -> Self {
            Self {
                id: new_account_id(),
                name: String::new(),
                account_type: AccountType::TraditionalIra,
                other_type_label: String::new(),
                starting_balance: "0".into(),
                cost_basis: "0".into(),
                annual_growth_rate: "6".into(),
                dividend_yield: "0".into(),
                contribution_allocation: "0".into(),
                reinvest_dividends: true,
            }
        }
    }

    #[derive(Clone, PartialEq, Eq)]
    struct SavedPlan {
        id: String,
        revision: u64,
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Operation {
        Idle,
        Loading,
        Saving,
        Projecting,
    }
    #[derive(Clone)]
    struct ApiResponse {
        status: u16,
        body: String,
    }
    #[derive(Clone, Copy)]
    struct Ui {
        profile: RwSignal<ProfileDraft>,
        accounts: RwSignal<Vec<AccountDraft>>,
        saved: RwSignal<Option<SavedPlan>>,
        projection: RwSignal<Option<ProjectionResponse>>,
        field_errors: RwSignal<Vec<FieldError>>,
        notice: RwSignal<Option<String>>,
        operation: RwSignal<Operation>,
        degraded: RwSignal<bool>,
        stale: RwSignal<bool>,
        missing: RwSignal<bool>,
        instance_reset: RwSignal<bool>,
    }

    fn new_account_id() -> String {
        let value = ACCOUNT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        format!("a11ce000-0000-4000-8000-{value:012x}")
    }
    fn account_type_value(value: AccountType) -> &'static str {
        match value {
            AccountType::TraditionalIra => "traditional_ira",
            AccountType::RothIra => "roth_ira",
            AccountType::Brokerage => "brokerage",
            AccountType::Employer401k => "employer_401k",
            AccountType::Cash => "cash",
            AccountType::Other => "other",
        }
    }
    fn account_type_from_value(value: &str) -> AccountType {
        match value {
            "roth_ira" => AccountType::RothIra,
            "brokerage" => AccountType::Brokerage,
            "employer_401k" => AccountType::Employer401k,
            "cash" => AccountType::Cash,
            "other" => AccountType::Other,
            _ => AccountType::TraditionalIra,
        }
    }

    fn dollars_to_cents(value: &str) -> Result<i64, ()> {
        i64::try_from(super::parse_hundredths(value)?).map_err(|_| ())
    }
    fn rate_to_bps(value: &str) -> Result<i32, ()> {
        i32::try_from(super::parse_hundredths(value)?).map_err(|_| ())
    }
    fn input_error(path: &str, message: &str) -> FieldError {
        FieldError {
            path: path.into(),
            code: "INVALID_INPUT".into(),
            message: message.into(),
        }
    }

    fn build_input(
        profile: &ProfileDraft,
        accounts: &[AccountDraft],
    ) -> Result<PlanInput, Vec<FieldError>> {
        let mut errors = Vec::new();
        let current_age = profile.current_age.trim().parse().unwrap_or_else(|_| {
            errors.push(input_error(
                "profile.current_age",
                "Enter a whole-number age.",
            ));
            0
        });
        let current_annual_income_cents =
            dollars_to_cents(&profile.annual_income).unwrap_or_else(|()| {
                errors.push(input_error(
                    "profile.current_annual_income_cents",
                    "Enter a valid dollar amount.",
                ));
                0
            });
        let projection_years = profile.projection_years.trim().parse().unwrap_or_else(|_| {
            errors.push(input_error(
                "profile.projection_years",
                "Enter a whole number of years.",
            ));
            0
        });
        let account_inputs = accounts
            .iter()
            .enumerate()
            .map(|(index, account)| {
                let money = |value: &str, field: &str, errors: &mut Vec<FieldError>| {
                    dollars_to_cents(value).unwrap_or_else(|()| {
                        errors.push(input_error(
                            &format!("accounts[{index}].{field}"),
                            "Enter a valid dollar amount.",
                        ));
                        0
                    })
                };
                let rate = |value: &str, field: &str, errors: &mut Vec<FieldError>| {
                    rate_to_bps(value).unwrap_or_else(|()| {
                        errors.push(input_error(
                            &format!("accounts[{index}].{field}"),
                            "Enter a valid percentage.",
                        ));
                        0
                    })
                };
                AccountInput {
                    id: account.id.clone(),
                    name: account.name.clone(),
                    account_type: account.account_type,
                    other_type_label: (account.account_type == AccountType::Other)
                        .then(|| account.other_type_label.clone()),
                    starting_balance_cents: money(
                        &account.starting_balance,
                        "starting_balance_cents",
                        &mut errors,
                    ),
                    cost_basis_cents: money(&account.cost_basis, "cost_basis_cents", &mut errors),
                    annual_growth_bps: rate(
                        &account.annual_growth_rate,
                        "annual_growth_bps",
                        &mut errors,
                    ),
                    annual_dividend_yield_bps: rate(
                        &account.dividend_yield,
                        "annual_dividend_yield_bps",
                        &mut errors,
                    ),
                    contribution_allocation_bps: rate(
                        &account.contribution_allocation,
                        "contribution_allocation_bps",
                        &mut errors,
                    ),
                    reinvest_dividends: account.reinvest_dividends,
                }
            })
            .collect();
        if errors.is_empty() {
            Ok(PlanInput {
                profile: PlanProfile {
                    current_age,
                    current_annual_income_cents,
                    projection_years,
                },
                accounts: account_inputs,
            })
        } else {
            Err(errors)
        }
    }

    fn cents_for_input(value: i64) -> String {
        format!("{}.{:02}", value / 100, value.unsigned_abs() % 100)
    }
    fn bps_for_input(value: i32) -> String {
        format!("{}.{:02}", value / 100, value.unsigned_abs() % 100)
    }
    fn draft_from_plan(plan: &PlanResponse) -> (ProfileDraft, Vec<AccountDraft>) {
        (
            ProfileDraft {
                current_age: plan.profile.current_age.to_string(),
                annual_income: cents_for_input(plan.profile.current_annual_income_cents),
                projection_years: plan.profile.projection_years.to_string(),
            },
            plan.accounts
                .iter()
                .map(|account| AccountDraft {
                    id: account.id.clone(),
                    name: account.name.clone(),
                    account_type: account.account_type,
                    other_type_label: account.other_type_label.clone().unwrap_or_default(),
                    starting_balance: cents_for_input(account.starting_balance_cents),
                    cost_basis: cents_for_input(account.cost_basis_cents),
                    annual_growth_rate: bps_for_input(account.annual_growth_bps),
                    dividend_yield: bps_for_input(account.annual_dividend_yield_bps),
                    contribution_allocation: bps_for_input(account.contribution_allocation_bps),
                    reinvest_dividends: account.reinvest_dividends,
                })
                .collect(),
        )
    }
    fn error_for(errors: &[FieldError], path: &str) -> Option<String> {
        errors
            .iter()
            .find(|error| error.path == path)
            .map(|error| error.message.clone())
    }
    fn server_error(response: &ApiResponse) -> ErrorResponse {
        serde_json::from_str(&response.body).unwrap_or_else(|_| ErrorResponse {
            code: "NETWORK_RESPONSE_ERROR".into(),
            message: format!("The server returned HTTP {}.", response.status),
            field_errors: vec![],
            request_id: String::new(),
        })
    }

    async fn request(
        method: &str,
        path: &str,
        body: Option<String>,
    ) -> Result<ApiResponse, String> {
        let options = RequestInit::new();
        options.set_method(method);
        if let Some(body) = body.as_ref() {
            options.set_body(&JsValue::from_str(body));
        }
        let request = Request::new_with_str_and_init(path, &options)
            .map_err(|_| "Could not prepare the request.".to_owned())?;
        if body.is_some() {
            request
                .headers()
                .set("content-type", "application/json")
                .map_err(|_| "Could not set request headers.".to_owned())?;
        }
        let window =
            web_sys::window().ok_or_else(|| "Browser window is unavailable.".to_owned())?;
        let response = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| "Could not reach the local calculator service.".to_owned())?
            .dyn_into::<Response>()
            .map_err(|_| "The local service returned an unreadable response.".to_owned())?;
        let status = response.status();
        let body = JsFuture::from(
            response
                .text()
                .map_err(|_| "Could not read the server response.".to_owned())?,
        )
        .await
        .map_err(|_| "Could not read the server response.".to_owned())?
        .as_string()
        .unwrap_or_default();
        Ok(ApiResponse { status, body })
    }
    fn set_failure(ui: Ui, response: &ApiResponse) {
        let error = server_error(response);
        ui.field_errors.set(error.field_errors);
        let suffix = if error.request_id.is_empty() {
            String::new()
        } else {
            format!(" Request ID: {}", error.request_id)
        };
        ui.notice.set(Some(format!("{}{}", error.message, suffix)));
        match response.status {
            404 => {
                ui.saved.set(None);
                ui.missing.set(true);
            }
            409 => ui.stale.set(true),
            503 => ui.degraded.set(true),
            _ => {}
        }
    }
    async fn observe_instance(ui: Ui) -> Result<(), String> {
        let response = request("GET", "/api/v1/health/live", None).await?;
        if response.status != 200 {
            return Err("The local calculator health check failed.".into());
        }
        let health: HealthResponse = serde_json::from_str(&response.body)
            .map_err(|_| "The local calculator health response was invalid.".to_owned())?;
        if let Some(storage) =
            web_sys::window().and_then(|window| window.session_storage().ok().flatten())
        {
            let previous = storage.get_item(INSTANCE_KEY).ok().flatten();
            if previous
                .as_deref()
                .is_some_and(|value| value != health.instance_id)
            {
                ui.saved.set(None);
                ui.instance_reset.set(true);
                ui.missing.set(true);
                ui.notice.set(Some("The local service restarted and its in-memory plans were cleared. Your form inputs are still here.".into()));
            }
            let _ = storage.set_item(INSTANCE_KEY, &health.instance_id);
        }
        Ok(())
    }
    async fn check_readiness(ui: Ui) {
        match request("GET", "/api/v1/health/ready", None).await {
            Ok(response) if response.status == 200 => ui.degraded.set(false),
            Ok(response) => {
                ui.degraded.set(true);
                set_failure(ui, &response);
            }
            Err(message) => {
                ui.degraded.set(true);
                ui.notice.set(Some(message));
            }
        }
    }
    fn validate(ui: Ui) -> Option<PlanInput> {
        ui.field_errors.set(vec![]);
        match build_input(&ui.profile.get_untracked(), &ui.accounts.get_untracked()) {
            Ok(input) => Some(input),
            Err(errors) => {
                ui.field_errors.set(errors);
                ui.notice.set(Some(
                    "Fix the highlighted fields. Your entries have been preserved.".into(),
                ));
                None
            }
        }
    }

    async fn save(ui: Ui, reapply: bool) {
        if ui.operation.get_untracked() != Operation::Idle {
            return;
        }
        ui.operation.set(Operation::Saving);
        ui.notice.set(None);
        if let Err(message) = observe_instance(ui).await {
            ui.notice.set(Some(message));
            ui.operation.set(Operation::Idle);
            return;
        }
        check_readiness(ui).await;
        if ui.degraded.get_untracked() {
            ui.notice.set(Some("Saving is unavailable while the in-memory store is locked. You can still project the current form.".into()));
            ui.operation.set(Operation::Idle);
            return;
        }
        let Some(input) = validate(ui) else {
            ui.operation.set(Operation::Idle);
            return;
        };
        let mut saved = ui.saved.get_untracked();
        if reapply {
            if let Some(current) = saved.as_ref() {
                match request("GET", &format!("/api/v1/plans/{}", current.id), None).await {
                    Ok(response) if response.status == 200 => {
                        if let Ok(plan) = serde_json::from_str::<PlanResponse>(&response.body) {
                            saved = Some(SavedPlan {
                                id: plan.plan_id,
                                revision: plan.revision,
                            });
                        }
                    }
                    Ok(response) => {
                        set_failure(ui, &response);
                        ui.operation.set(Operation::Idle);
                        return;
                    }
                    Err(message) => {
                        ui.notice.set(Some(message));
                        ui.operation.set(Operation::Idle);
                        return;
                    }
                }
            }
        }
        let (method, path, body) = if let Some(current) = saved {
            let update = UpdatePlanInput {
                expected_revision: current.revision,
                profile: input.profile,
                accounts: input.accounts,
            };
            (
                "PUT",
                format!("/api/v1/plans/{}", current.id),
                serde_json::to_string(&update),
            )
        } else {
            (
                "POST",
                "/api/v1/plans".into(),
                serde_json::to_string(&input),
            )
        };
        let response = body.map_err(|_| "Could not encode the plan.".to_owned());
        match response {
            Ok(body) => match request(method, &path, Some(body)).await {
                Ok(response) if matches!(response.status, 200 | 201) => {
                    match serde_json::from_str::<PlanResponse>(&response.body) {
                        Ok(plan) => {
                            ui.saved.set(Some(SavedPlan {
                                id: plan.plan_id,
                                revision: plan.revision,
                            }));
                            ui.stale.set(false);
                            ui.missing.set(false);
                            ui.instance_reset.set(false);
                            ui.notice
                                .set(Some("Plan saved in this local service session.".into()));
                        }
                        Err(_) => ui.notice.set(Some(
                            "The server returned an invalid saved-plan response.".into(),
                        )),
                    }
                }
                Ok(response) => set_failure(ui, &response),
                Err(message) => ui.notice.set(Some(message)),
            },
            Err(message) => ui.notice.set(Some(message)),
        }
        ui.operation.set(Operation::Idle);
    }
    async fn reload_saved(ui: Ui) {
        let Some(saved) = ui.saved.get_untracked() else {
            return;
        };
        ui.operation.set(Operation::Loading);
        match request("GET", &format!("/api/v1/plans/{}", saved.id), None).await {
            Ok(response) if response.status == 200 => {
                match serde_json::from_str::<PlanResponse>(&response.body) {
                    Ok(plan) => {
                        let (profile, accounts) = draft_from_plan(&plan);
                        ui.profile.set(profile);
                        ui.accounts.set(accounts);
                        ui.saved.set(Some(SavedPlan {
                            id: plan.plan_id,
                            revision: plan.revision,
                        }));
                        ui.field_errors.set(vec![]);
                        ui.stale.set(false);
                        ui.notice.set(Some("Reloaded the latest saved version. Unsaved conflicting edits were replaced.".into()));
                    }
                    Err(_) => ui
                        .notice
                        .set(Some("The saved plan response was invalid.".into())),
                }
            }
            Ok(response) => set_failure(ui, &response),
            Err(message) => ui.notice.set(Some(message)),
        }
        ui.operation.set(Operation::Idle);
    }
    async fn project(ui: Ui) {
        if ui.operation.get_untracked() != Operation::Idle {
            return;
        }
        ui.operation.set(Operation::Projecting);
        ui.notice.set(None);
        if let Err(message) = observe_instance(ui).await {
            ui.notice.set(Some(message));
        }
        let Some(input) = validate(ui) else {
            ui.operation.set(Operation::Idle);
            return;
        };
        match serde_json::to_string(&input) {
            Ok(body) => match request("POST", "/api/v1/projections", Some(body)).await {
                Ok(response) if response.status == 200 => {
                    match serde_json::from_str::<ProjectionResponse>(&response.body) {
                        Ok(projection) => {
                            ui.projection.set(Some(projection));
                            ui.field_errors.set(vec![]);
                            ui.notice.set(Some(
                                "Projection updated from the current form inputs.".into(),
                            ));
                        }
                        Err(_) => ui
                            .notice
                            .set(Some("The projection response was invalid.".into())),
                    }
                }
                Ok(response) => set_failure(ui, &response),
                Err(message) => ui.notice.set(Some(message)),
            },
            Err(_) => ui
                .notice
                .set(Some("Could not encode the projection.".into())),
        }
        ui.operation.set(Operation::Idle);
    }

    fn update_account(
        accounts: RwSignal<Vec<AccountDraft>>,
        index: usize,
        update: impl FnOnce(&mut AccountDraft),
    ) {
        accounts.update(|items| {
            if let Some(account) = items.get_mut(index) {
                update(account);
            }
        });
    }
    fn field_error_view(errors: RwSignal<Vec<FieldError>>, path: String) -> impl IntoView {
        move || {
            error_for(&errors.get(), &path)
                .map(|message| view! { <p class="field-error" role="alert">{message}</p> })
        }
    }

    #[component]
    fn MoneyField<F>(
        id: String,
        label: &'static str,
        value: String,
        path: String,
        errors: RwSignal<Vec<FieldError>>,
        on_input: F,
    ) -> impl IntoView
    where
        F: Fn(String) + 'static,
    {
        let error_id = format!("{id}-error");
        view! { <div class="field"><label for=id.clone()>{label}</label><div class="input-suffix"><span aria-hidden="true">"$"</span><input id=id inputmode="decimal" value=value on:input=move |event| on_input(event_target_value(&event)) aria-describedby=error_id.clone() /></div><div id=error_id>{field_error_view(errors, path)}</div></div> }
    }
    #[component]
    fn RateField<F>(
        id: String,
        label: &'static str,
        value: String,
        path: String,
        errors: RwSignal<Vec<FieldError>>,
        on_input: F,
    ) -> impl IntoView
    where
        F: Fn(String) + 'static,
    {
        let error_id = format!("{id}-error");
        view! { <div class="field"><label for=id.clone()>{label}</label><div class="input-suffix suffix-right"><input id=id inputmode="decimal" value=value on:input=move |event| on_input(event_target_value(&event)) aria-describedby=error_id.clone() /><span aria-hidden="true">"%"</span></div><div id=error_id>{field_error_view(errors, path)}</div></div> }
    }

    #[component]
    fn AccountCard(index: usize, account: AccountDraft, ui: Ui) -> impl IntoView {
        let prefix = format!("accounts[{index}]");
        let id_prefix = format!("account-{index}");
        let name = account.name.clone();
        let starting = account.starting_balance.clone();
        let basis = account.cost_basis.clone();
        let growth = account.annual_growth_rate.clone();
        let dividend = account.dividend_yield.clone();
        let allocation = account.contribution_allocation.clone();
        view! { <article class="account-card" data-account-index=index>
            <div class="account-heading"><div><span class="eyebrow">{format!("Account {}", index + 1)}</span><h3>{if name.is_empty() { "Untitled account".into() } else { name.clone() }}</h3></div><div class="account-actions">
                <button type="button" class="button-quiet" on:click=move |_| { let mut copy = ui.accounts.get_untracked().get(index).cloned().unwrap_or_else(AccountDraft::blank); copy.id = new_account_id(); copy.name = if copy.name.is_empty() { "Copy".into() } else { format!("{} copy", copy.name) }; ui.accounts.update(|items| items.insert(index + 1, copy)); }>"Duplicate"</button>
                <button type="button" class="button-danger" aria-label=format!("Remove account {}", index + 1) on:click=move |_| ui.accounts.update(|items| { if index < items.len() { items.remove(index); } })>"Remove"</button></div></div>
            <div class="field-grid"><div class="field span-2"><label for=format!("{id_prefix}-name")>"Account name"</label><input id=format!("{id_prefix}-name") value=name on:input=move |event| update_account(ui.accounts, index, |item| item.name = event_target_value(&event)) />{field_error_view(ui.field_errors, format!("{prefix}.name"))}</div>
            <div class="field span-2"><label for=format!("{id_prefix}-type")>"Account type"</label><select id=format!("{id_prefix}-type") prop:value=account_type_value(account.account_type) on:change=move |event| { let value = event.target().and_then(|target| target.dyn_into::<HtmlSelectElement>().ok()).map(|select| select.value()).unwrap_or_default(); update_account(ui.accounts, index, |item| item.account_type = account_type_from_value(&value)); }><option value="traditional_ira">"Traditional IRA"</option><option value="roth_ira">"Roth IRA"</option><option value="brokerage">"Brokerage"</option><option value="employer_401k">"Employer 401(k)"</option><option value="cash">"Cash"</option><option value="other">"Other"</option></select></div>
            {if account.account_type == AccountType::Other { Some(view! { <div class="field span-2"><label for=format!("{id_prefix}-other")>"Other account type"</label><input id=format!("{id_prefix}-other") value=account.other_type_label on:input=move |event| update_account(ui.accounts, index, |item| item.other_type_label = event_target_value(&event)) />{field_error_view(ui.field_errors, format!("{prefix}.other_type_label"))}</div> }) } else { None }}
            <MoneyField id=format!("{id_prefix}-balance") label="Starting balance" value=starting path=format!("{prefix}.starting_balance_cents") errors=ui.field_errors on_input=move |value| update_account(ui.accounts, index, |item| item.starting_balance = value) />
            <MoneyField id=format!("{id_prefix}-basis") label="Cost basis" value=basis path=format!("{prefix}.cost_basis_cents") errors=ui.field_errors on_input=move |value| update_account(ui.accounts, index, |item| item.cost_basis = value) />
            <RateField id=format!("{id_prefix}-growth") label="Annual growth" value=growth path=format!("{prefix}.annual_growth_bps") errors=ui.field_errors on_input=move |value| update_account(ui.accounts, index, |item| item.annual_growth_rate = value) />
            <RateField id=format!("{id_prefix}-yield") label="Dividend yield" value=dividend path=format!("{prefix}.annual_dividend_yield_bps") errors=ui.field_errors on_input=move |value| update_account(ui.accounts, index, |item| item.dividend_yield = value) />
            <RateField id=format!("{id_prefix}-allocation") label="Contribution allocation" value=allocation path=format!("{prefix}.contribution_allocation_bps") errors=ui.field_errors on_input=move |value| update_account(ui.accounts, index, |item| item.contribution_allocation = value) />
            <label class="toggle span-2"><input type="checkbox" checked=account.reinvest_dividends on:change=move |event| { let checked = event.target().and_then(|target| target.dyn_into::<HtmlInputElement>().ok()).is_some_and(|input| input.checked()); update_account(ui.accounts, index, |item| item.reinvest_dividends = checked); } /><span>"Reinvest dividends"</span></label></div>
        </article> }
    }

    fn chart_forced_failure() -> bool {
        web_sys::window()
            .and_then(|window| window.location().search().ok())
            .is_some_and(|query| query.contains("chart=fail"))
    }
    #[component]
    fn ResultCard(label: &'static str, value: String) -> impl IntoView {
        view! { <article class="result-card"><span>{label}</span><strong>{value}</strong></article> }
    }
    #[component]
    fn Results(projection: ProjectionResponse) -> impl IntoView {
        let final_row = projection.years.last().map(|year| &year.portfolio);
        let ending = final_row.map_or(0, |row| row.closing_balance_cents);
        let contributions = projection
            .years
            .iter()
            .map(|year| year.portfolio.contribution_cents)
            .sum();
        let appreciation = projection
            .years
            .iter()
            .map(|year| year.portfolio.appreciation_cents)
            .sum();
        let income = final_row.map_or(0, |row| row.cumulative_income_paid_cents);
        let max_value = projection
            .years
            .iter()
            .map(|year| year.portfolio.closing_balance_cents.max(0))
            .max()
            .unwrap_or(1)
            .max(1);
        let denominator = projection.years.len().max(2) - 1;
        let points = projection
            .years
            .iter()
            .enumerate()
            .map(|(index, year)| {
                let x = index * 1_000 / denominator;
                let balance = i128::from(year.portfolio.closing_balance_cents.max(0));
                let y = 960 - balance * 880 / i128::from(max_value);
                format!("{x},{y}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        view! { <section class="results" aria-labelledby="results-heading"><div class="section-heading"><div><span class="eyebrow">"Projection"</span><h2 id="results-heading">"Your outlook"</h2></div><p>"Estimates, not financial advice."</p></div>
            <div class="result-cards"><ResultCard label="Ending value" value=format_money(ending) /><ResultCard label="Contributions" value=format_money(contributions) /><ResultCard label="Appreciation" value=format_money(appreciation) /><ResultCard label="Cash income" value=format_money(income) /></div>
            <div class="chart-panel"><h3>"Value over time"</h3>{if chart_forced_failure() || points.is_empty() { view! { <div class="chart-fallback" role="status"><strong>"Chart unavailable"</strong><span>"The complete annual results remain available below."</span></div> }.into_any() } else { view! { <svg class="chart" viewBox="0 0 1000 1000" role="img" aria-labelledby="chart-title chart-desc" preserveAspectRatio="none"><title id="chart-title">"Projected portfolio value over time"</title><desc id="chart-desc">{format!("Ending value {} over {} years.", format_money(ending), projection.years.len())}</desc><polyline points=points fill="none" vector-effect="non-scaling-stroke" /></svg> }.into_any() }}</div>
            <div class="table-scroll" tabindex="0" aria-label="Scrollable annual projection table"><table><caption>"Annual portfolio projection"</caption><thead><tr><th scope="col">"Year"</th><th scope="col">"Age"</th><th scope="col">"Opening"</th><th scope="col">"Contributions"</th><th scope="col">"Appreciation"</th><th scope="col">"Dividends"</th><th scope="col">"Cash income"</th><th scope="col">"Ending"</th></tr></thead><tbody>{projection.years.into_iter().map(|year| view! { <tr><th scope="row">{year.year}</th><td>{year.age}</td><td>{format_money(year.portfolio.opening_balance_cents)}</td><td>{format_money(year.portfolio.contribution_cents)}</td><td>{format_money(year.portfolio.appreciation_cents)}</td><td>{format_money(year.portfolio.dividend_generated_cents)}</td><td>{format_money(year.portfolio.income_paid_cents)}</td><td>{format_money(year.portfolio.closing_balance_cents)}</td></tr> }).collect_view()}</tbody></table></div>
        </section> }
    }

    #[component]
    pub fn App() -> impl IntoView {
        let ui = Ui {
            profile: RwSignal::new(ProfileDraft::default()),
            accounts: RwSignal::new(vec![AccountDraft::blank()]),
            saved: RwSignal::new(None),
            projection: RwSignal::new(None),
            field_errors: RwSignal::new(vec![]),
            notice: RwSignal::new(None),
            operation: RwSignal::new(Operation::Loading),
            degraded: RwSignal::new(false),
            stale: RwSignal::new(false),
            missing: RwSignal::new(false),
            instance_reset: RwSignal::new(false),
        };
        spawn_local(async move {
            if let Err(message) = observe_instance(ui).await {
                ui.notice.set(Some(message));
            }
            check_readiness(ui).await;
            ui.operation.set(Operation::Idle);
        });
        let allocation = move || {
            ui.accounts
                .get()
                .iter()
                .filter_map(|item| rate_to_bps(&item.contribution_allocation).ok())
                .sum::<i32>()
        };
        let busy = move || ui.operation.get() != Operation::Idle;
        view! { <a class="skip-link" href="#planner">"Skip to planner"</a><header class="site-header"><div class="brand"><span class="brand-mark" aria-hidden="true">"R"</span><div><strong>"Retirement Planner"</strong><span>"Local scenario workspace"</span></div></div><span class="trust-badge">"Trusted device only"</span></header>
        <main id="planner"><section class="hero"><div><span class="eyebrow">"Plan with clarity"</span><h1>"Build a retirement scenario you can inspect."</h1><p>"Compare account assumptions and see a year-by-year projection. Data is held only by this local, single-tenant service and clears when it restarts."</p></div><aside><strong>"Prototype disclosure"</strong><p>"For local use by a trusted person. Not production-ready and not financial advice."</p></aside></section>
        <Show when=move || ui.operation.get() == Operation::Loading><div class="notice" role="status" aria-live="polite">"Loading the local service state…"</div></Show>
        <Show when=move || ui.degraded.get()><div class="banner banner-warning" role="status"><strong>"Saving temporarily unavailable"</strong><span>"The in-memory store is locked. Form inputs and stateless projections remain available."</span></div></Show>
        <Show when=move || ui.instance_reset.get()><div class="banner banner-warning" role="alert"><strong>"Service memory was reset"</strong><span>"Your form was preserved, but its saved identity was invalidated."</span><button type="button" on:click=move |_| spawn_local(save(ui, false))>"Recreate plan"</button></div></Show>
        <Show when=move || ui.missing.get() && !ui.instance_reset.get()><div class="banner banner-warning" role="alert"><strong>"Saved plan is missing"</strong><span>"It may have been deleted or cleared. Your form is intact."</span><button type="button" on:click=move |_| spawn_local(save(ui, false))>"Create replacement"</button></div></Show>
        <Show when=move || ui.stale.get()><div class="banner banner-warning" role="alert"><strong>"A newer saved version exists"</strong><span>"Your edits remain in the form."</span><div><button type="button" class="button-secondary" on:click=move |_| spawn_local(reload_saved(ui))>"Reload saved"</button><button type="button" on:click=move |_| spawn_local(save(ui, true))>"Reapply my changes"</button></div></div></Show>
        <Show when=move || ui.notice.get().is_some()><div class="notice" role="status" aria-live="polite">{move || ui.notice.get()}</div></Show>
        <section class="planner-grid"><div class="editor"><section class="panel" aria-labelledby="profile-heading"><div class="section-heading"><div><span class="eyebrow">"Step 1"</span><h2 id="profile-heading">"Your profile"</h2></div></div><div class="field-grid profile-grid">
            <div class="field"><label for="current-age">"Current age"</label><input id="current-age" inputmode="numeric" prop:value=move || ui.profile.get().current_age on:input=move |event| ui.profile.update(|profile| profile.current_age = event_target_value(&event)) />{field_error_view(ui.field_errors, "profile.current_age".into())}</div>
            <MoneyField id="annual-income".into() label="Current annual income" value=ui.profile.get_untracked().annual_income path="profile.current_annual_income_cents".into() errors=ui.field_errors on_input=move |value| ui.profile.update(|profile| profile.annual_income = value) />
            <div class="field"><label for="projection-years">"Projection years"</label><div class="input-suffix suffix-right"><input id="projection-years" inputmode="numeric" prop:value=move || ui.profile.get().projection_years on:input=move |event| ui.profile.update(|profile| profile.projection_years = event_target_value(&event)) /><span aria-hidden="true">"years"</span></div>{field_error_view(ui.field_errors, "profile.projection_years".into())}</div>
        </div></section>
        <section class="accounts-section" aria-labelledby="accounts-heading"><div class="section-heading"><div><span class="eyebrow">"Step 2"</span><h2 id="accounts-heading">"Accounts"</h2></div><button type="button" class="button-secondary" on:click=move |_| ui.accounts.update(|accounts| accounts.push(AccountDraft::blank()))>"+ Add account"</button></div>
        <div class="allocation" aria-live="polite"><div><strong>"Contribution allocation"</strong><span>{move || format!("{:.2}% used · {:.2}% remaining", f64::from(allocation()) / 100.0, f64::from(10_000 - allocation()) / 100.0)}</span></div><progress max="10000" value=move || allocation().clamp(0, 10_000)></progress></div>
        <Show when=move || ui.accounts.get().is_empty() fallback=move || ui.accounts.get().into_iter().enumerate().map(|(index, account)| view! { <AccountCard index=index account=account ui=ui /> }).collect_view()><div class="empty-state"><h3>"No accounts yet"</h3><p>"Add an account to save a draft or calculate a projection."</p><button type="button" on:click=move |_| ui.accounts.update(|accounts| accounts.push(AccountDraft::blank()))>"Add your first account"</button></div></Show></section>
        <Show when=move || ui.projection.get().is_some()>{move || ui.projection.get().map(|projection| view! { <Results projection=projection /> })}</Show></div>
        <aside class="action-rail" aria-label="Plan actions"><div class="action-card"><span class="eyebrow">"Step 3"</span><h2>"Save or project"</h2><p>{move || ui.saved.get().map_or_else(|| "Not saved in this service session.".into(), |saved| format!("Saved locally at revision {}.", saved.revision))}</p><button type="button" class="button-secondary button-full" disabled=move || busy() || ui.degraded.get() on:click=move |_| spawn_local(save(ui, false))>{move || if ui.operation.get() == Operation::Saving { "Saving…" } else if ui.saved.get().is_some() { "Save changes" } else { "Save plan" }}</button><button type="button" class="button-primary button-full" disabled=busy on:click=move |_| spawn_local(project(ui))>{move || if ui.operation.get() == Operation::Projecting { "Projecting…" } else { "Project retirement" }}</button><small>"Projection uses the current form, even before saving."</small></div></aside></section></main>
        <footer>"Local prototype · single tenant · no production usability claim"</footer> }
    }
    fn event_target_value(event: &ev::Event) -> String {
        event
            .target()
            .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
            .map(|input| input.value())
            .unwrap_or_default()
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    leptos::mount::mount_to_body(browser::App);
}

#[cfg(test)]
mod tests {
    #[test]
    fn browser_contract_decoder_accepts_the_golden_health_fixture() {
        let fixture = include_str!("../../../crates/api-contract/tests/fixtures/v1/health.json");
        assert_eq!(
            super::decode_health(fixture)
                .expect("health fixture")
                .status,
            "ok"
        );
    }
    #[test]
    fn money_formatting_is_stable_and_display_only() {
        assert_eq!(super::format_money(0), "$0.00");
        assert_eq!(super::format_money(123_456_789), "$1,234,567.89");
        assert_eq!(super::format_money(-501), "-$5.01");
        assert_eq!(super::format_money(i64::MIN), "-$92,233,720,368,547,758.08");
    }

    #[test]
    fn decimal_inputs_convert_without_float_rounding() {
        assert_eq!(super::parse_hundredths("1"), Ok(100));
        assert_eq!(super::parse_hundredths("-0.05"), Ok(-5));
        assert_eq!(super::parse_hundredths(".5"), Ok(50));
        assert!(super::parse_hundredths("1.234").is_err());
        assert!(super::parse_hundredths("NaN").is_err());
    }
}
