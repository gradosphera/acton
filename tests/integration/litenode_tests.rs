use crate::common::assertion;
use crate::support::project::ProjectBuilder;
use crate::support::snapshots::normalize_output_preserve_escapes;

const SIMPLE_CONTRACT: &str = r"
fun onInternalMessage(in: InMessage) {}
fun onBouncedMessage(_: InMessageBounced) {}
";

#[test]
fn litenode_supports_pre_start_commands_and_get_out_msg_queue_size() {
    let project = ProjectBuilder::new("litenode-pre-start-commands")
        .contract("simple", SIMPLE_CONTRACT)
        .script_file(
            "prepare",
            r#"
                import "../../lib/io"

                fun main() {
                    println("pre-start setup done");
                }
            "#,
        )
        .build();

    let node = project
        .litenode()
        .before_start(|cmd| cmd.build())
        .before_start(|cmd| cmd.script("scripts/prepare.tolk"))
        .start();

    let response = node.get_json("/api/v2/getOutMsgQueueSize");
    let mut response = response;
    response["@extra"] = serde_json::json!("[EXTRA]");

    let response_json =
        serde_json::to_string_pretty(&response).expect("Failed to serialize JSON response");

    assertion().eq(
        normalize_output_preserve_escapes(&response_json, project.path()),
        snapbox::file!("snapshots/test_litenode_get_out_msg_queue_size.response.json"),
    );
}
