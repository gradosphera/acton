use crate::support::TestOutputExt;
use crate::support::project::ProjectBuilder;
use function_name::named;
use std::process::Command;

const CONTRACT: &str = r#"
            fun onInternalMessage(_in: InMessage) {
                val value = 1;
                debug.print(value);
            }
        "#;

const CST_PLUGIN: &str = r#"
export default function run(input) {
  const { cst } = input ?? {};
  if (!cst || !Array.isArray(cst.children)) {
    throw new Error("CST is not available");
  }

  return [
    {
      message: "js plugin cst probe",
      severity: "warning",
      start: cst.startByte,
      end: cst.startByte + 1,
      help: "plugin can inspect tree-sitter CST",
    },
  ];
}
"#;

const TREE_SITTER_STYLE_PLUGIN: &str = r#"
export default function lint(ctx) {
  const root = ctx?.tree?.rootNode;
  if (!root) {
    throw new Error("tree.rootNode is not available");
  }

  const functions = root.descendantsOfType("function_declaration");
  const target = functions.find((fn) => fn.childForFieldName("name")?.text === "onInternalMessage");
  if (!target) {
    return [];
  }

  const nameNode = target.childForFieldName("name");
  return [
    {
      message: `tree-sitter api probe: ${nameNode.text}`,
      severity: "warning",
      start: nameNode.startIndex,
      end: nameNode.endIndex,
      help: `function kind: ${target.type}`,
    },
  ];
}
"#;

#[test]
#[named]
fn check_lint_js_plugin_receives_cst() {
    if !has_node() {
        eprintln!("Skipping test: Node.js is not available");
        return;
    }

    let project = ProjectBuilder::new(&format!("check-{}", function_name!()))
        .contract("main", CONTRACT)
        .raw_file("plugins/cst-plugin.mjs", CST_PLUGIN)
        .with_lint_js_plugin("plugins/cst-plugin.mjs")
        .build();

    project.acton().init().run().success();

    project
        .acton()
        .check()
        .run()
        .success()
        .assert_stderr_snapshot_matches(&format!(
            "integration/snapshots/check/lint_js_plugins/{}.txt",
            function_name!()
        ));
}

#[test]
#[named]
fn check_lint_js_plugin_tree_sitter_style_api() {
    if !has_node() {
        eprintln!("Skipping test: Node.js is not available");
        return;
    }

    let project = ProjectBuilder::new(&format!("check-{}", function_name!()))
        .contract("main", CONTRACT)
        .raw_file(
            "plugins/tree-sitter-style-plugin.mjs",
            TREE_SITTER_STYLE_PLUGIN,
        )
        .with_lint_js_plugin("plugins/tree-sitter-style-plugin.mjs")
        .build();

    project.acton().init().run().success();

    project
        .acton()
        .check()
        .run()
        .success()
        .assert_stderr_snapshot_matches(&format!(
            "integration/snapshots/check/lint_js_plugins/{}.txt",
            function_name!()
        ));
}

fn has_node() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
