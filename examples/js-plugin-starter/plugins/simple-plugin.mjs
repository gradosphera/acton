// @ts-check

/**
 * @param {import("./acton-plugin-api").LintContext} ctx
 * @returns {import("./acton-plugin-api").PluginDiagnostic[]}
 */
export default function lint(ctx) {
  const root = ctx?.tree?.rootNode;
  if (!root) {
    return [];
  }

  const functions = root.descendantsOfType("function_declaration");
  const targetFunction = functions.find(
    (fn) => fn.childForFieldName("name")?.text === "onInternalMessage",
  );
  if (!targetFunction) {
    return [];
  }

  const nameNode = targetFunction.childForFieldName("name");
  if (!nameNode) {
    return [];
  }

  return [
    {
      message: `Demo JS plugin: found function ${nameNode.text}`,
      severity: "warning",
      start: nameNode.startIndex,
      end: nameNode.endIndex,
      help: `Tree-sitter style API is available for ${ctx.filePath}.`,
    },
  ];
}
