use crate::support::TestOutputExt;
use crate::support::project::ProjectBuilder;
use function_name::named;

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

const REGISTERED_PLUGIN: &str = r#"
export default {
  register() {
    return {
      name: "registered-plugin",
      rules: {
        "no-debug-print": {
          code: "JSP001",
          title: "avoid debug.print in runtime paths",
          description: "debug.print can expose runtime internals",
          help: "remove debug.print or gate it behind explicit debug mode",
          severity: "warning",
        },
      },
    };
  },
  lint(ctx) {
    const root = ctx?.tree?.rootNode;
    if (!root) {
      return [];
    }

    const calls = root.descendantsOfType("function_call");
    const target = calls.find((node) => node.text.includes("debug.print("));
    if (!target) {
      return [];
    }

    return [
      {
        ruleId: "no-debug-print",
        start: target.startIndex,
        end: target.endIndex,
      },
    ];
  },
};
"#;

const TYPE_API_PLUGIN: &str = r#"
export default {
  register() {
    return {
      name: "type-api-plugin",
      rules: {
        "expr-type-probe": {
          code: "JSP002",
          title: "expression type probe",
          severity: "warning",
        },
      },
    };
  },
  lint(ctx) {
    const root = ctx?.tree?.rootNode;
    if (!root) {
      return [];
    }

    const target = root.descendantsOfType("number_literal")[0];
    if (!target) {
      return [];
    }

    const fromCtx = ctx.typeOf(target);
    const fromNode = target.inferredType;
    return [
      {
        ruleId: "expr-type-probe",
        message: `type api: ${fromCtx ?? "none"} / ${fromNode ?? "none"}`,
        start: target.startIndex,
        end: target.endIndex,
      },
    ];
  },
};
"#;

#[test]
#[named]
fn check_lint_js_plugin_receives_cst() {
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

#[test]
#[named]
fn check_lint_js_plugin_registration_metadata() {
    let project = ProjectBuilder::new(&format!("check-{}", function_name!()))
        .contract("main", CONTRACT)
        .raw_file("plugins/registered-plugin.mjs", REGISTERED_PLUGIN)
        .with_lint_js_plugin("plugins/registered-plugin.mjs")
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
fn check_lint_js_plugin_expression_type_api() {
    let project = ProjectBuilder::new(&format!("check-{}", function_name!()))
        .contract("main", CONTRACT)
        .raw_file("plugins/type-api-plugin.mjs", TYPE_API_PLUGIN)
        .with_lint_js_plugin("plugins/type-api-plugin.mjs")
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
