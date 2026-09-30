use std::{fs, path::Path};

use api_contract::{ErrorResponse, HealthResponse, PlanInput, PlanResponse, ProjectionResponse};

const FIXTURES: &[&str] = &[
    "health.json",
    "create-plan.json",
    "update-plan.json",
    "stale-update.json",
    "stored-projection.json",
    "stateless-projection.json",
    "repeated-delete.json",
];

#[test]
fn all_frozen_v1_fixtures_are_valid_json() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1");
    for fixture in FIXTURES {
        let bytes = fs::read(root.join(fixture)).expect("golden fixture must be readable");
        let _: serde_json::Value =
            serde_json::from_slice(&bytes).expect("golden fixture must contain valid JSON");
    }
}

#[test]
fn success_and_error_fixtures_decode_to_explicit_contract_types() {
    let create: PlanResponse =
        serde_json::from_str(include_str!("fixtures/v1/create-plan.json")).expect("create fixture");
    let update: PlanResponse =
        serde_json::from_str(include_str!("fixtures/v1/update-plan.json")).expect("update fixture");
    let stale: ErrorResponse =
        serde_json::from_str(include_str!("fixtures/v1/stale-update.json")).expect("stale fixture");
    let stored: ProjectionResponse =
        serde_json::from_str(include_str!("fixtures/v1/stored-projection.json"))
            .expect("stored fixture");
    let stateless: ProjectionResponse =
        serde_json::from_str(include_str!("fixtures/v1/stateless-projection.json"))
            .expect("stateless fixture");
    assert_eq!(create.revision, 1);
    assert_eq!(update.revision, 2);
    assert_eq!(stale.code, "REVISION_CONFLICT");
    assert!(stored.plan_ref.is_some());
    assert!(stateless.plan_ref.is_none());
    assert_eq!(stored.years, stateless.years);
}

#[test]
fn projection_fixture_serialization_is_byte_stable() {
    let fixture = include_str!("fixtures/v1/stateless-projection.json").trim();
    let projection: ProjectionResponse = serde_json::from_str(fixture).expect("fixture");
    assert_eq!(
        serde_json::to_string(&projection).expect("serialize"),
        fixture
    );
}

#[test]
fn mutation_contract_rejects_unknown_fields() {
    let input = r#"{"profile":{"current_age":40,"current_annual_income_cents":0,"projection_years":1},"accounts":[],"misspelled_income":1}"#;
    assert!(serde_json::from_str::<PlanInput>(input).is_err());
}

#[test]
fn health_fixture_decodes_to_the_public_contract() {
    let fixture = include_str!("fixtures/v1/health.json");
    let health: HealthResponse = serde_json::from_str(fixture).expect("health fixture must decode");
    assert_eq!(health.status, "ok");
    assert_eq!(health.build_version, env!("CARGO_PKG_VERSION"));
}
