use crate::FixAvailability;
use crate::rules::violation::Violation;
use tolk_macros::ViolationMetadata;

/// ### What it does
/// Carries diagnostics emitted by external JavaScript lint plugins.
///
/// ### Behavior notes
/// - This rule is used as a transport for plugin diagnostics in `acton check`.
/// - Plugin diagnostics may point to CST-derived spans.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.1")]
pub struct JsPlugin;

impl Violation for JsPlugin {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    fn message(&self) -> String {
        "diagnostic emitted by JavaScript plugin".to_string()
    }
}
