//! Application ports and use cases.

/// Confirms the application layer points inward to the domain.
#[must_use]
pub fn boundary_name() -> &'static str {
    domain::boundary_name()
}
