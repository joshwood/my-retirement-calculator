//! Versioned HTTP boundary data transfer objects.

use serde::{Deserialize, Serialize};

/// Response shared by the v1 live and ready health endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HealthResponse {
    pub status: String,
    pub build_version: String,
    pub instance_id: String,
}
