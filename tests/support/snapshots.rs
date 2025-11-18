use crate::common::{assert_ui, strip_ansi};
use crate::regex;
use snapbox::IntoData;
use snapbox::filter::Filter;
use std::path::PathBuf;

pub fn normalize_output(stdout: &str, project_path: &PathBuf) -> String {
    let content = strip_ansi(stdout).into_data();
    let content = snapbox::filter::FilterPaths.filter(content.into_data());
    let content = snapbox::filter::FilterNewlines.filter(content);
    let content = content.render().expect("came in as a String");

    let assert1 = assert_ui();
    let mut redactions = assert1.redactions().clone();

    let tmp_dir = project_path.to_string_lossy().to_string();
    redactions.insert("[ROOT]", tmp_dir.clone()).unwrap();
    redactions
        .insert(
            "[DATE]",
            regex!(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}[+-]\d{2}:\d{2}"),
        )
        .unwrap();
    redactions
        .insert("[DURATION]", regex!(r"duration='\d+'"))
        .unwrap();
    redactions
        .insert("[TIME]", regex!(r#"time="\d+\.\d+""#))
        .unwrap();
    redactions
        .insert("[ROOT]", "/private".to_owned() + tmp_dir.as_str())
        .unwrap();

    redactions.redact(&content)
}
