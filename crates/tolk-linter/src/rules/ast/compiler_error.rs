use crate::FixAvailability;
use crate::rules::violation::Violation;
use tolk_macros::ViolationMetadata;

/// ### What it does
/// Reports any compiler error.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.1")]
pub struct CompilerError;

impl Violation for CompilerError {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    fn message(&self) -> String {
        "compiler error".to_string()
    }
}
