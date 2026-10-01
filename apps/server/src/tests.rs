use std::{path::PathBuf, thread};

use adapters_memory::MemoryPlanRepository;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::{bind_address, router, router_with_repository};

const INSTANCE_ID: &str = "00000000-0000-4000-8000-000000000001";
const ACCOUNT_ID: &str = "10000000-0000-4000-8000-000000000001";

fn public_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../public")
}
fn input() -> Value {
    json!({"profile":{"current_age":40,"current_annual_income_cents":10_000_000,"projection_years":2},"accounts":[{"id":ACCOUNT_ID,"name":"Sensitive Account Name","account_type":"roth_ira","other_type_label":null,"starting_balance_cents":10_000_000,"cost_basis_cents":7_000_000,"annual_growth_bps":600,"annual_dividend_yield_bps":500,"contribution_allocation_bps":1000,"reinvest_dividends":true}]})
}
fn json_request(method: &str, uri: &str, value: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(value).expect("serialize")))
        .expect("request")
}

#[test]
fn port_defaults_to_8080_on_all_interfaces() {
    assert_eq!(
        bind_address(None).expect("default address").to_string(),
        "0.0.0.0:8080"
    );
}

#[test]
fn port_accepts_valid_u16() {
    assert_eq!(
        bind_address(Some("49152"))
            .expect("configured address")
            .to_string(),
        "0.0.0.0:49152"
    );
}

#[test]
fn port_rejects_invalid_values() {
    for port in ["", "not-a-port", "65536", "-1"] {
        assert!(bind_address(Some(port)).is_err(), "accepted PORT={port:?}");
    }
}
async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .expect("body"),
    )
    .expect("json")
}

#[tokio::test]
async fn crud_stale_update_and_idempotent_delete() {
    let app = router(public_dir(), INSTANCE_ID.into());
    let created_response = app
        .clone()
        .oneshot(json_request("POST", "/api/v1/plans", &input()))
        .await
        .expect("create");
    assert_eq!(created_response.status(), StatusCode::CREATED);
    let created = json_body(created_response).await;
    let plan_id = created["plan_id"].as_str().expect("id");
    assert_eq!(created["revision"], 1);

    let fetched = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/plans/{plan_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("get");
    assert_eq!(fetched.status(), StatusCode::OK);

    let mut update = input();
    update
        .as_object_mut()
        .expect("object")
        .insert("expected_revision".into(), json!(1));
    let updated = app
        .clone()
        .oneshot(json_request(
            "PUT",
            &format!("/api/v1/plans/{plan_id}"),
            &update,
        ))
        .await
        .expect("update");
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(json_body(updated).await["revision"], 2);

    let stale = app
        .clone()
        .oneshot(json_request(
            "PUT",
            &format!("/api/v1/plans/{plan_id}"),
            &update,
        ))
        .await
        .expect("stale");
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(stale).await["code"], "REVISION_CONFLICT");
    let current = json_body(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/plans/{plan_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("get"),
    )
    .await;
    assert_eq!(current["revision"], 2);

    for _ in 0..2 {
        let deleted = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/v1/plans/{plan_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("delete");
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            to_bytes(deleted.into_body(), 1).await.expect("body").len(),
            0
        );
    }
}

#[tokio::test]
async fn invalid_and_unknown_inputs_do_not_mutate() {
    let app = router(public_dir(), INSTANCE_ID.into());
    let mut invalid = input();
    invalid["profile"]["current_age"] = json!(10);
    let response = app
        .clone()
        .oneshot(json_request("POST", "/api/v1/plans", &invalid))
        .await
        .expect("invalid");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = json_body(response).await;
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert_eq!(error["field_errors"][0]["path"], "profile.current_age");
    let mut unknown = input();
    unknown
        .as_object_mut()
        .expect("object")
        .insert("secret_amount".into(), json!(999));
    let response = app
        .clone()
        .oneshot(json_request("POST", "/api/v1/plans", &unknown))
        .await
        .expect("unknown");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = json_body(response).await;
    assert_eq!(error["code"], "VALIDATION_ERROR");
    assert_eq!(error["field_errors"][0]["path"], "secret_amount");
    let metrics = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("metrics");
    let text = String::from_utf8(
        to_bytes(metrics.into_body(), 64 * 1024)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(text.contains("retirement_plans 0"));
}

#[tokio::test]
async fn stored_and_stateless_projection_are_stable_and_equivalent() {
    let app = router(public_dir(), INSTANCE_ID.into());
    let created = json_body(
        app.clone()
            .oneshot(json_request("POST", "/api/v1/plans", &input()))
            .await
            .expect("create"),
    )
    .await;
    let plan_id = created["plan_id"].as_str().expect("id");
    let stateless_a = json_body(
        app.clone()
            .oneshot(json_request("POST", "/api/v1/projections", &input()))
            .await
            .expect("projection"),
    )
    .await;
    let stateless_b = json_body(
        app.clone()
            .oneshot(json_request("POST", "/api/v1/projections", &input()))
            .await
            .expect("projection"),
    )
    .await;
    assert_eq!(stateless_a, stateless_b);
    assert!(stateless_a["plan_ref"].is_null());
    let stored = json_body(
        app.oneshot(json_request(
            "POST",
            &format!("/api/v1/plans/{plan_id}/projections"),
            &json!({}),
        ))
        .await
        .expect("stored projection"),
    )
    .await;
    assert_eq!(stored["plan_ref"]["revision"], 1);
    let mut normalized = stored;
    normalized["plan_ref"] = Value::Null;
    assert_eq!(normalized, stateless_a);
}

#[tokio::test]
async fn health_alias_matches_liveness_without_repository_mutation() {
    let repository = MemoryPlanRepository::new();
    let app = router_with_repository(public_dir(), INSTANCE_ID.into(), repository.clone());
    let alias = json_body(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health alias"),
    )
    .await;
    let liveness = json_body(
        app.oneshot(
            Request::builder()
                .uri("/api/v1/health/live")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("liveness"),
    )
    .await;

    assert_eq!(alias, liveness);
    assert!(
        repository
            .shared_store()
            .read()
            .expect("repository lock")
            .is_empty()
    );
}

#[tokio::test]
async fn health_identity_readiness_poison_and_security_headers() {
    let repository = MemoryPlanRepository::new();
    let app = router_with_repository(public_dir(), INSTANCE_ID.into(), repository.clone());
    for route in ["/health", "/api/v1/health/live", "/api/v1/health/ready"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(route)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["instance_id"], INSTANCE_ID);
    }
    let store = repository.shared_store();
    let _ = thread::spawn(move || {
        let _guard = store.write().expect("lock");
        panic!("intentional poison");
    })
    .join();
    let live = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/live")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("live");
    assert_eq!(live.status(), StatusCode::OK);
    let ready = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/ready")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("ready");
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(json_body(ready).await["code"], "SERVICE_UNAVAILABLE");
    let repository_request = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/plans/20000000-0000-4000-8000-000000000001")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("repository request");
    assert_eq!(repository_request.status(), StatusCode::SERVICE_UNAVAILABLE);
    let asset = app
        .oneshot(
            Request::builder()
                .uri("/assets/v1/app.css")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("asset");
    assert_eq!(asset.headers()["x-content-type-options"], "nosniff");
    assert!(asset.headers().contains_key("content-security-policy"));
    assert!(!asset.headers().contains_key("access-control-allow-origin"));
    let restarted = router(public_dir(), "different-process".into())
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/live")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("restart health");
    assert_ne!(json_body(restarted).await["instance_id"], INSTANCE_ID);
}

#[tokio::test]
async fn metrics_are_aggregate_and_body_limit_is_enforced() {
    let app = router(public_dir(), INSTANCE_ID.into());
    let _ = app
        .clone()
        .oneshot(json_request("POST", "/api/v1/projections", &input()))
        .await
        .expect("projection");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("metrics");
    let text = String::from_utf8(
        to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("utf8");
    assert!(text.contains("retirement_projections_total 1"));
    for sensitive in ["Sensitive Account Name", ACCOUNT_ID, "10000000", "7000000"] {
        assert!(!text.contains(sensitive));
    }
    let oversized = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/plans")
                .header("content-type", "application/json")
                .body(Body::from(vec![b'x'; 1024 * 1024 + 1]))
                .expect("request"),
        )
        .await
        .expect("limited");
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
