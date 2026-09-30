//! Leptos CSR entry point and v1 contract decoder.

use api_contract::HealthResponse;

/// Decode the health response through the shared browser/server DTO contract.
///
/// # Errors
///
/// Returns a JSON decoding error when the response does not match the frozen v1
/// health contract.
pub fn decode_health(json: &str) -> Result<HealthResponse, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use leptos::prelude::*;

    let golden = include_str!("../../../crates/api-contract/tests/fixtures/v1/health.json");
    let message = decode_health(golden).map_or_else(
        |_| "health fixture decode failed".to_owned(),
        |health| format!("health fixture decoded: {}", health.status),
    );

    mount_to_body(move || {
        view! {
            <main>
                <h1>"Retirement Calculator"</h1>
                <p id="health-status">{message.clone()}</p>
            </main>
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn browser_contract_decoder_accepts_the_golden_health_fixture() {
        let fixture = include_str!("../../../crates/api-contract/tests/fixtures/v1/health.json");
        let health = super::decode_health(fixture).expect("golden health must decode");
        assert_eq!(health.status, "ok");
    }
}
