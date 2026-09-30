//! Pure retirement-calculator domain.
//!
//! The executable projection model lands in the next implementation stage. This
//! crate deliberately has no dependencies so its boundary is mechanically clear.

/// Identifies this crate in foundation-level dependency tests.
#[must_use]
pub const fn boundary_name() -> &'static str {
    "domain"
}

#[cfg(test)]
mod tests {
    #[test]
    fn domain_is_independently_testable() {
        assert_eq!(super::boundary_name(), "domain");
    }
}
