// @ts-check

/**
 * @param {import("./acton-plugin-api").LintContext} ctx
 * @returns {import("./acton-plugin-api").PluginDiagnostic[]}
 */
function lint(ctx) {
  const root = ctx?.tree?.rootNode;
  if (!root) {
    return [];
  }

  const assignments = root.descendantsOfType("assignment");
  let variableNameNode = null;
  let initializerNode = null;
  for (const assignment of assignments) {
    const left = assignment.childForFieldName("left");
    const right = assignment.childForFieldName("right");
    if (!left || !right) {
      continue;
    }

    const declaration = left.descendantsOfType("var_declaration")[0];
    const nameNode = declaration?.childForFieldName("name");
    if (nameNode?.text === "profile") {
      variableNameNode = nameNode;
      initializerNode = right;
      break;
    }
  }

  if (!variableNameNode || !initializerNode) {
    return [];
  }

  const profileType = ctx.typeOf(initializerNode) ?? initializerNode.inferredType ?? "unknown";

  return [
    {
      ruleId: "entrypoint-name-probe",
      start: variableNameNode.startIndex,
      end: variableNameNode.endIndex,
      description: `Variable ${variableNameNode.text} has inferred type ${profileType}.`,
    },
  ];
}

export default {
  register() {
    return {
      name: "starter-plugin",
      description: "Demo plugin with registered metadata.",
      rules: {
        "entrypoint-name-probe": {
          code: "JSP001",
          title: "Struct literal variable type probe",
          description:
            "The plugin finds a local variable by name and reads the inferred type of its struct literal initializer.",
          help: "Resolve assignment -> right side and call ctx.typeOf(node) to get expression type.",
          severity: "warning",
          docsUrl: "https://example.com/docs/jsp001",
        },
      },
    };
  },
  lint,
};
