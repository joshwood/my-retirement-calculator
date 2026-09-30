//! Process-local repository adapter scaffolding.

/// Confirms the adapter has only the approved inward edges.
#[must_use]
pub fn boundary_names() -> (&'static str, &'static str) {
    (application::boundary_name(), domain::boundary_name())
}
