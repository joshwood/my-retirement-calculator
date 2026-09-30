//! Axum composition root for the local-only retirement calculator.

use std::path::Path;

use api_contract::HealthResponse;
use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue},
    routing::get,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
};

#[derive(Clone)]
struct AppState {
    health: HealthResponse,
}

/// Builds the application router with API and versioned browser assets.
pub fn router(asset_root: impl AsRef<Path>, instance_id: String) -> Router {
    let asset_root = asset_root.as_ref();
    let state = AppState {
        health: HealthResponse {
            status: "ok".into(),
            build_version: env!("CARGO_PKG_VERSION").into(),
            instance_id,
        },
    };
    let request_id = HeaderName::from_static("x-request-id");

    // Touch each approved dependency from this composition root. Later stages
    // replace these boundary probes with actual use-case construction.
    let _ = (
        adapters_memory::boundary_names(),
        application::boundary_name(),
        domain::boundary_name(),
    );

    Router::new()
        .route("/api/v1/health/live", get(live))
        .nest_service(
            "/assets/v1",
            ServeDir::new(asset_root.join("assets/v1")),
        )
        .fallback_service(ServeFile::new(asset_root.join("index.html")))
        .with_state(state)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(SetResponseHeaderLayer::if_not_present(
            http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            http::header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            http::header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'",
            ),
        ))
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}

async fn live(axum::extract::State(state): axum::extract::State<AppState>) -> Json<HealthResponse> {
    Json(state.health)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::router;

    const INSTANCE_ID: &str = "00000000-0000-4000-8000-000000000001";

    fn public_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../public")
    }

    #[tokio::test]
    async fn live_health_matches_the_golden_fixture() {
        let response = router(public_dir(), INSTANCE_ID.into())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health/live")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health response");
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers()["content-security-policy"],
            "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .expect("health body");
        let actual: serde_json::Value = serde_json::from_slice(&bytes).expect("health JSON");
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../crates/api-contract/tests/fixtures/v1/health.json"
        ))
        .expect("golden health JSON");
        assert_eq!(actual, golden);
    }

    #[tokio::test]
    async fn versioned_assets_are_served_with_security_headers_and_no_cors() {
        let response = router(public_dir(), INSTANCE_ID.into())
            .oneshot(
                Request::builder()
                    .uri("/assets/v1/app.css")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("asset response");
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(response.headers().contains_key("x-request-id"));
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
    }
}
