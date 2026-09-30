use std::{fs, path::Path};

use api_contract::HealthResponse;

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
fn all_frozen_v1_scaffolds_are_valid_json() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1");
    for fixture in FIXTURES {
        let bytes = fs::read(root.join(fixture)).expect("golden fixture must be readable");
        let _: serde_json::Value =
            serde_json::from_slice(&bytes).expect("golden fixture must contain valid JSON");
    }
}

#[test]
fn health_fixture_decodes_to_the_public_contract() {
    let fixture = include_str!("fixtures/v1/health.json");
    let health: HealthResponse = serde_json::from_str(fixture).expect("health fixture must decode");
    assert_eq!(health.status, "ok");
    assert_eq!(health.build_version, env!("CARGO_PKG_VERSION"));
}
