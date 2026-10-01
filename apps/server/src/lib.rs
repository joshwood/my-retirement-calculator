//! Axum composition root for the retirement calculator.

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    num::ParseIntError,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use adapters_memory::MemoryPlanRepository;
use api_contract as dto;
use application::{
    Application, ApplicationError, CreatePlan, DeletePlan, GetPlan, PlanRepository, ProjectPlan,
    RepositoryError, UpdatePlan,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, MatchedPath, Path as AxumPath, State, rejection::JsonRejection},
    http::{HeaderName, HeaderValue, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use domain::{Account, AccountId, PlanId};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};
use uuid::Uuid;

const MAX_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_PORT: u16 = 8080;

/// Resolves the public listen address from an optional `PORT` value.
///
/// Parsing happens before the listener is created so an invalid value fails
/// startup instead of silently falling back to the default.
///
/// # Errors
///
/// Returns a parsing error when `port` is not a valid `u16`.
pub fn bind_address(port: Option<&str>) -> Result<SocketAddr, ParseIntError> {
    let port = port.map_or(Ok(DEFAULT_PORT), str::parse::<u16>)?;
    Ok(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        port,
    )))
}

#[derive(Debug, Default)]
struct Metrics {
    requests: AtomicU64,
    request_errors: AtomicU64,
    request_duration_micros: AtomicU64,
    projections: AtomicU64,
    projection_failures: AtomicU64,
    projection_duration_micros: AtomicU64,
}

#[derive(Clone)]
struct AppState {
    application: Application<MemoryPlanRepository>,
    health: dto::HealthResponse,
    metrics: Arc<Metrics>,
}

#[derive(Clone)]
struct RequestId(String);

pub fn router(asset_root: impl AsRef<Path>, instance_id: String) -> Router {
    router_with_repository(asset_root, instance_id, MemoryPlanRepository::new())
}

pub fn router_with_repository(
    asset_root: impl AsRef<Path>,
    instance_id: String,
    repository: MemoryPlanRepository,
) -> Router {
    let asset_root = asset_root.as_ref();
    let state = AppState {
        application: Application::new(repository),
        health: dto::HealthResponse {
            status: "ok".into(),
            build_version: env!("CARGO_PKG_VERSION").into(),
            instance_id,
        },
        metrics: Arc::new(Metrics::default()),
    };

    Router::new()
        .route("/api/v1/plans", post(create_plan))
        .route("/api/v1/plans/{plan_id}", get(get_plan).put(update_plan).delete(delete_plan))
        .route("/api/v1/plans/{plan_id}/projections", post(project_stored))
        .route("/api/v1/projections", post(project_stateless))
        .route("/health", get(live))
        .route("/api/v1/health/live", get(live))
        .route("/api/v1/health/ready", get(ready))
        .route("/metrics", get(metrics))
        .nest_service("/assets/v1", ServeDir::new(asset_root.join("assets/v1")))
        .fallback_service(ServeFile::new(asset_root.join("index.html")))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::if_not_present(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer")))
        .layer(SetResponseHeaderLayer::if_not_present(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'")))
        .layer(middleware::from_fn_with_state(state.clone(), track_request))
        .with_state(state)
}

async fn track_request(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = request.method().as_str().to_owned();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str)
        .to_owned();
    let request_id = Uuid::new_v4().to_string();
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-request-id"), value);
    }
    let elapsed = started.elapsed().as_micros();
    state.metrics.requests.fetch_add(1, Ordering::Relaxed);
    state.metrics.request_duration_micros.fetch_add(
        u64::try_from(elapsed).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    if response.status().is_client_error() || response.status().is_server_error() {
        state.metrics.request_errors.fetch_add(1, Ordering::Relaxed);
    }
    tracing::info!(
        event = "request_completed",
        method,
        route,
        status = response.status().as_u16(),
        latency_micros = elapsed,
        request_id
    );
    response
}

async fn create_plan(
    State(state): State<AppState>,
    request_id: axum::Extension<RequestId>,
    body: Result<Json<dto::PlanInput>, JsonRejection>,
) -> Response {
    let input = match json_or_error(body, &request_id.0.0) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let (profile, accounts) = match input_to_domain(input) {
        Ok(value) => value,
        Err(errors) => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
                "Request validation failed",
                errors,
                &request_id.0.0,
            );
        }
    };
    match state.application.create(CreatePlan {
        id: PlanId::new(Uuid::new_v4()),
        profile,
        accounts,
        now: SystemTime::now(),
    }) {
        Ok(plan) => {
            repository_event("create", "success");
            (StatusCode::CREATED, Json(plan_to_dto(&plan))).into_response()
        }
        Err(error) => application_error(error, &request_id.0.0, "create"),
    }
}

async fn get_plan(
    State(state): State<AppState>,
    request_id: axum::Extension<RequestId>,
    AxumPath(plan_id): AxumPath<String>,
) -> Response {
    let id = match parse_plan_id(&plan_id, &request_id.0.0) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match state.application.get(GetPlan { id }) {
        Ok(plan) => {
            repository_event("get", "success");
            Json(plan_to_dto(&plan)).into_response()
        }
        Err(error) => application_error(error, &request_id.0.0, "get"),
    }
}

async fn update_plan(
    State(state): State<AppState>,
    request_id: axum::Extension<RequestId>,
    AxumPath(plan_id): AxumPath<String>,
    body: Result<Json<dto::UpdatePlanInput>, JsonRejection>,
) -> Response {
    let id = match parse_plan_id(&plan_id, &request_id.0.0) {
        Ok(id) => id,
        Err(response) => return response,
    };
    let input = match json_or_error(body, &request_id.0.0) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let expected_revision = input.expected_revision;
    let (profile, accounts) = match input_to_domain(dto::PlanInput {
        profile: input.profile,
        accounts: input.accounts,
    }) {
        Ok(value) => value,
        Err(errors) => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
                "Request validation failed",
                errors,
                &request_id.0.0,
            );
        }
    };
    match state.application.update(UpdatePlan {
        id,
        expected_revision,
        profile,
        accounts,
        now: SystemTime::now(),
    }) {
        Ok(plan) => {
            repository_event("update", "success");
            Json(plan_to_dto(&plan)).into_response()
        }
        Err(error) => application_error(error, &request_id.0.0, "update"),
    }
}

async fn delete_plan(
    State(state): State<AppState>,
    request_id: axum::Extension<RequestId>,
    AxumPath(plan_id): AxumPath<String>,
) -> Response {
    let id = match parse_plan_id(&plan_id, &request_id.0.0) {
        Ok(id) => id,
        Err(response) => return response,
    };
    match state.application.delete(DeletePlan { id }) {
        Ok(()) => {
            repository_event("delete", "success");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => application_error(error, &request_id.0.0, "delete"),
    }
}

async fn project_stored(
    State(state): State<AppState>,
    request_id: axum::Extension<RequestId>,
    AxumPath(plan_id): AxumPath<String>,
    body: Result<Json<dto::StoredProjectionInput>, JsonRejection>,
) -> Response {
    if let Err(response) = json_or_error(body, &request_id.0.0) {
        return response;
    }
    let id = match parse_plan_id(&plan_id, &request_id.0.0) {
        Ok(id) => id,
        Err(response) => return response,
    };
    project_response(&state, ProjectPlan::Stored { id }, &request_id.0.0)
}

async fn project_stateless(
    State(state): State<AppState>,
    request_id: axum::Extension<RequestId>,
    body: Result<Json<dto::PlanInput>, JsonRejection>,
) -> Response {
    let input = match json_or_error(body, &request_id.0.0) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let (profile, accounts) = match input_to_domain(input) {
        Ok(value) => value,
        Err(errors) => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
                "Request validation failed",
                errors,
                &request_id.0.0,
            );
        }
    };
    project_response(
        &state,
        ProjectPlan::Stateless { profile, accounts },
        &request_id.0.0,
    )
}

fn project_response(state: &AppState, query: ProjectPlan, request_id: &str) -> Response {
    let started = Instant::now();
    state.metrics.projections.fetch_add(1, Ordering::Relaxed);
    match state.application.project(query) {
        Ok(result) => {
            let duration = started.elapsed().as_micros();
            state.metrics.projection_duration_micros.fetch_add(
                u64::try_from(duration).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
            tracing::info!(
                event = "projection_completed",
                outcome = "success",
                latency_micros = duration,
                account_count = result.projection.accounts.len(),
                year_count = result.projection.years.len()
            );
            Json(projection_to_dto(result)).into_response()
        }
        Err(error) => {
            state
                .metrics
                .projection_failures
                .fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                event = "projection_completed",
                outcome = "failure",
                failure_code = application_error_code(&error)
            );
            application_error(error, request_id, "project")
        }
    }
}

async fn live(State(state): State<AppState>) -> Json<dto::HealthResponse> {
    Json(state.health)
}

async fn ready(State(state): State<AppState>, request_id: axum::Extension<RequestId>) -> Response {
    match state.application.repository().ready() {
        Ok(()) => Json(state.health).into_response(),
        Err(error) => application_error(
            ApplicationError::Repository(error),
            &request_id.0.0,
            "ready",
        ),
    }
}

async fn metrics(State(state): State<AppState>) -> Response {
    let plans = state.application.repository().count().unwrap_or(0);
    let requests = state.metrics.requests.load(Ordering::Relaxed);
    let request_duration_seconds = prometheus_seconds(
        state
            .metrics
            .request_duration_micros
            .load(Ordering::Relaxed),
    );
    let projections = state.metrics.projections.load(Ordering::Relaxed);
    let projection_duration_seconds = prometheus_seconds(
        state
            .metrics
            .projection_duration_micros
            .load(Ordering::Relaxed),
    );
    let body = format!(
        "# TYPE retirement_http_requests_total counter\nretirement_http_requests_total {requests}\n# TYPE retirement_http_request_errors_total counter\nretirement_http_request_errors_total {}\n# TYPE retirement_http_request_duration_seconds histogram\nretirement_http_request_duration_seconds_bucket{{le=\"+Inf\"}} {requests}\nretirement_http_request_duration_seconds_sum {request_duration_seconds}\nretirement_http_request_duration_seconds_count {requests}\n# TYPE retirement_projections_total counter\nretirement_projections_total {projections}\n# TYPE retirement_projection_failures_total counter\nretirement_projection_failures_total {}\n# TYPE retirement_projection_duration_seconds histogram\nretirement_projection_duration_seconds_bucket{{le=\"+Inf\"}} {projections}\nretirement_projection_duration_seconds_sum {projection_duration_seconds}\nretirement_projection_duration_seconds_count {projections}\n# TYPE retirement_plans gauge\nretirement_plans {plans}\n",
        state.metrics.request_errors.load(Ordering::Relaxed),
        state.metrics.projection_failures.load(Ordering::Relaxed),
    );
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

fn prometheus_seconds(microseconds: u64) -> String {
    format!(
        "{}.{:06}",
        microseconds / 1_000_000,
        microseconds % 1_000_000
    )
}

#[allow(clippy::result_large_err)]
fn json_or_error<T>(body: Result<Json<T>, JsonRejection>, request_id: &str) -> Result<T, Response> {
    body.map(|Json(value)| value).map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "REQUEST_TOO_LARGE",
                "Request body exceeds the configured limit",
                vec![],
                request_id,
            )
        } else if let Some(path) = unknown_field(&rejection.body_text()) {
            error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
                "Request validation failed",
                vec![dto::FieldError {
                    path,
                    code: "UNKNOWN_FIELD".into(),
                    message: "Field is not part of the v1 contract".into(),
                }],
                request_id,
            )
        } else {
            error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_JSON",
                "Request body must match the v1 contract",
                vec![],
                request_id,
            )
        }
    })
}

fn unknown_field(message: &str) -> Option<String> {
    let value = message.split("unknown field `").nth(1)?.split('`').next()?;
    (!value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_'))
    .then(|| value.to_owned())
}

#[allow(clippy::result_large_err)]
fn parse_plan_id(value: &str, request_id: &str) -> Result<PlanId, Response> {
    Uuid::parse_str(value).map(PlanId::new).map_err(|_| {
        error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR",
            "Request validation failed",
            vec![dto::FieldError {
                path: "plan_id".into(),
                code: "INVALID_FORMAT".into(),
                message: "Must be a UUID".into(),
            }],
            request_id,
        )
    })
}

fn input_to_domain(
    input: dto::PlanInput,
) -> Result<(domain::PlanProfile, Vec<Account>), Vec<dto::FieldError>> {
    let profile = domain::PlanProfile {
        current_age: input.profile.current_age,
        current_annual_income_cents: input.profile.current_annual_income_cents,
        projection_years: input.profile.projection_years,
    };
    let mut errors = Vec::new();
    let accounts = input
        .accounts
        .into_iter()
        .enumerate()
        .filter_map(|(index, account)| {
            if let Ok(id) = Uuid::parse_str(&account.id) {
                Some(Account {
                    id: AccountId::new(id),
                    name: account.name,
                    account_type: account_type_to_domain(account.account_type),
                    other_type_label: account.other_type_label,
                    starting_balance_cents: account.starting_balance_cents,
                    cost_basis_cents: account.cost_basis_cents,
                    annual_growth_bps: account.annual_growth_bps,
                    annual_dividend_yield_bps: account.annual_dividend_yield_bps,
                    contribution_allocation_bps: account.contribution_allocation_bps,
                    reinvest_dividends: account.reinvest_dividends,
                })
            } else {
                errors.push(dto::FieldError {
                    path: format!("accounts[{index}].id"),
                    code: "INVALID_FORMAT".into(),
                    message: "Must be a UUID".into(),
                });
                None
            }
        })
        .collect();
    if errors.is_empty() {
        Ok((profile, accounts))
    } else {
        Err(errors)
    }
}

fn account_type_to_domain(value: dto::AccountType) -> domain::AccountType {
    match value {
        dto::AccountType::TraditionalIra => domain::AccountType::TraditionalIra,
        dto::AccountType::RothIra => domain::AccountType::RothIra,
        dto::AccountType::Brokerage => domain::AccountType::Brokerage,
        dto::AccountType::Employer401k => domain::AccountType::Employer401k,
        dto::AccountType::Cash => domain::AccountType::Cash,
        dto::AccountType::Other => domain::AccountType::Other,
    }
}
fn account_type_to_dto(value: domain::AccountType) -> dto::AccountType {
    match value {
        domain::AccountType::TraditionalIra => dto::AccountType::TraditionalIra,
        domain::AccountType::RothIra => dto::AccountType::RothIra,
        domain::AccountType::Brokerage => dto::AccountType::Brokerage,
        domain::AccountType::Employer401k => dto::AccountType::Employer401k,
        domain::AccountType::Cash => dto::AccountType::Cash,
        domain::AccountType::Other => dto::AccountType::Other,
    }
}

fn profile_to_dto(value: &domain::PlanProfile) -> dto::PlanProfile {
    dto::PlanProfile {
        current_age: value.current_age,
        current_annual_income_cents: value.current_annual_income_cents,
        projection_years: value.projection_years,
    }
}
fn account_to_dto(value: &Account) -> dto::AccountInput {
    dto::AccountInput {
        id: value.id.value().to_string(),
        name: value.name.clone(),
        account_type: account_type_to_dto(value.account_type),
        other_type_label: value.other_type_label.clone(),
        starting_balance_cents: value.starting_balance_cents,
        cost_basis_cents: value.cost_basis_cents,
        annual_growth_bps: value.annual_growth_bps,
        annual_dividend_yield_bps: value.annual_dividend_yield_bps,
        contribution_allocation_bps: value.contribution_allocation_bps,
        reinvest_dividends: value.reinvest_dividends,
    }
}
fn timestamp(value: SystemTime) -> String {
    value
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
        .to_string()
}

fn plan_to_dto(plan: &domain::RetirementPlan) -> dto::PlanResponse {
    dto::PlanResponse {
        contract_version: dto::CONTRACT_VERSION.into(),
        plan_id: plan.id.value().to_string(),
        revision: plan.revision,
        created_at: timestamp(plan.created_at),
        updated_at: timestamp(plan.updated_at),
        profile: profile_to_dto(&plan.profile),
        accounts: plan.accounts.iter().map(account_to_dto).collect(),
    }
}

fn projection_to_dto(value: application::ProjectedPlan) -> dto::ProjectionResponse {
    let plan_ref = value.stored_plan.as_ref().map(|plan| dto::PlanReference {
        plan_id: plan.id.value().to_string(),
        revision: plan.revision,
    });
    dto::ProjectionResponse {
        contract_version: dto::CONTRACT_VERSION.into(),
        plan_ref,
        profile: profile_to_dto(&value.projection.profile),
        accounts: value
            .projection
            .accounts
            .iter()
            .map(|account| dto::AccountInput {
                id: account.id.value().to_string(),
                name: account.name.clone(),
                account_type: account_type_to_dto(account.account_type),
                other_type_label: account.other_type_label.clone(),
                starting_balance_cents: account.starting_balance_cents,
                cost_basis_cents: account.cost_basis_cents,
                annual_growth_bps: account.annual_growth_bps,
                annual_dividend_yield_bps: account.annual_dividend_yield_bps,
                contribution_allocation_bps: account.contribution_allocation_bps,
                reinvest_dividends: account.reinvest_dividends,
            })
            .collect(),
        years: value
            .projection
            .years
            .into_iter()
            .map(|year| dto::ProjectionYear {
                year: year.year,
                age: year.age,
                accounts: year
                    .accounts
                    .into_iter()
                    .map(|row| dto::AccountProjectionRow {
                        account_id: row.account_id.value().to_string(),
                        opening_balance_cents: row.opening_balance_cents,
                        contribution_cents: row.contribution_cents,
                        invested_balance_cents: row.invested_balance_cents,
                        appreciation_cents: row.appreciation_cents,
                        dividend_generated_cents: row.dividend_generated_cents,
                        dividend_reinvested_cents: row.dividend_reinvested_cents,
                        income_paid_cents: row.income_paid_cents,
                        closing_balance_cents: row.closing_balance_cents,
                        cumulative_income_paid_cents: row.cumulative_income_paid_cents,
                    })
                    .collect(),
                portfolio: dto::PortfolioProjectionRow {
                    opening_balance_cents: year.portfolio.opening_balance_cents,
                    contribution_cents: year.portfolio.contribution_cents,
                    invested_balance_cents: year.portfolio.invested_balance_cents,
                    appreciation_cents: year.portfolio.appreciation_cents,
                    dividend_generated_cents: year.portfolio.dividend_generated_cents,
                    dividend_reinvested_cents: year.portfolio.dividend_reinvested_cents,
                    income_paid_cents: year.portfolio.income_paid_cents,
                    closing_balance_cents: year.portfolio.closing_balance_cents,
                    cumulative_income_paid_cents: year.portfolio.cumulative_income_paid_cents,
                },
            })
            .collect(),
        warnings: vec![],
    }
}

fn application_error(
    error: ApplicationError,
    request_id: &str,
    operation: &'static str,
) -> Response {
    if matches!(&error, ApplicationError::Repository(_)) {
        repository_event(operation, "failure");
    }
    match error {
        ApplicationError::Validation(errors)
        | ApplicationError::Projection(domain::ProjectionError::Validation(errors)) => {
            error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALIDATION_ERROR",
                "Request validation failed",
                errors
                    .into_vec()
                    .into_iter()
                    .map(|error| dto::FieldError {
                        path: error.path,
                        code: validation_code(error.code).into(),
                        message: error.message.into(),
                    })
                    .collect(),
                request_id,
            )
        }
        ApplicationError::Projection(domain::ProjectionError::Calculation(_)) => error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "CALCULATION_RANGE_EXCEEDED",
            "Calculation range exceeded",
            vec![],
            request_id,
        ),
        ApplicationError::Repository(RepositoryError::NotFound) => error_response(
            StatusCode::NOT_FOUND,
            "PLAN_NOT_FOUND",
            "Plan not found",
            vec![],
            request_id,
        ),
        ApplicationError::Repository(RepositoryError::RevisionConflict { .. }) => error_response(
            StatusCode::CONFLICT,
            "REVISION_CONFLICT",
            "Plan revision conflict",
            vec![],
            request_id,
        ),
        ApplicationError::Repository(RepositoryError::Unavailable) => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "Service temporarily unavailable",
            vec![],
            request_id,
        ),
        ApplicationError::Repository(RepositoryError::AlreadyExists) => error_response(
            StatusCode::CONFLICT,
            "PLAN_ALREADY_EXISTS",
            "Plan already exists",
            vec![],
            request_id,
        ),
    }
}

fn application_error_code(error: &ApplicationError) -> &'static str {
    match error {
        ApplicationError::Validation(_)
        | ApplicationError::Projection(domain::ProjectionError::Validation(_)) => {
            "VALIDATION_ERROR"
        }
        ApplicationError::Projection(domain::ProjectionError::Calculation(_)) => {
            "CALCULATION_RANGE_EXCEEDED"
        }
        ApplicationError::Repository(RepositoryError::NotFound) => "PLAN_NOT_FOUND",
        ApplicationError::Repository(RepositoryError::RevisionConflict { .. }) => {
            "REVISION_CONFLICT"
        }
        ApplicationError::Repository(RepositoryError::Unavailable) => "SERVICE_UNAVAILABLE",
        ApplicationError::Repository(RepositoryError::AlreadyExists) => "PLAN_ALREADY_EXISTS",
    }
}
fn validation_code(code: domain::ValidationCode) -> &'static str {
    match code {
        domain::ValidationCode::OutOfRange => "OUT_OF_RANGE",
        domain::ValidationCode::Required => "REQUIRED",
        domain::ValidationCode::Unexpected => "UNEXPECTED",
        domain::ValidationCode::Duplicate => "DUPLICATE",
        domain::ValidationCode::TooMany => "TOO_MANY",
        domain::ValidationCode::AggregateOutOfRange => "AGGREGATE_OUT_OF_RANGE",
    }
}
fn repository_event(operation: &'static str, outcome: &'static str) {
    tracing::info!(event = "repository_operation", operation, outcome);
}
fn error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    field_errors: Vec<dto::FieldError>,
    request_id: &str,
) -> Response {
    (
        status,
        Json(dto::ErrorResponse {
            code: code.into(),
            message: message.into(),
            field_errors,
            request_id: request_id.into(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
