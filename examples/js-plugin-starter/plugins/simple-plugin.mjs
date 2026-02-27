// @ts-check

const RUNTIME_ENTRYPOINTS = new Set([
  "main",
  "onInternalMessage",
  "onExternalMessage",
]);
const ALLOWED_NUMERIC_LITERALS = new Set(["0", "1"]);

/**
 * @param {import("./acton-plugin-api").SyntaxNode} callNode
 * @returns {boolean}
 */
function isDebugPrintCall(callNode) {
  const text = callNode?.text;
  return typeof text === "string" && /^debug\.print\s*\(/.test(text);
}

/**
 * @param {import("./acton-plugin-api").SyntaxNode} numberNode
 * @returns {boolean}
 */
function isAllowedNumericLiteral(numberNode) {
  const raw = numberNode?.text;
  if (typeof raw !== "string") {
    return true;
  }
  const normalized = raw.replaceAll("_", "");
  return ALLOWED_NUMERIC_LITERALS.has(normalized);
}

/**
 * @param {import("./acton-plugin-api").LintContext} ctx
 * @returns {import("./acton-plugin-api").PluginDiagnostic[]}
 */
function lint(ctx) {
  const root = ctx?.tree?.rootNode;
  if (!root) {
    return [];
  }

  /** @type {import("./acton-plugin-api").PluginDiagnostic[]} */
  const diagnostics = [];
  const functions = root.descendantsOfType("function_declaration");
  for (const fn of functions) {
    const functionName = fn.childForFieldName("name");
    if (!functionName || !RUNTIME_ENTRYPOINTS.has(functionName.text)) {
      continue;
    }

    const calls = fn.descendantsOfType("function_call");
    for (const call of calls) {
      if (!isDebugPrintCall(call)) {
        continue;
      }

      diagnostics.push({
        ruleId: "no-debug-print",
        message: `Avoid debug.print in ${functionName.text}`,
        start: call.startIndex,
        end: call.endIndex,
      });
    }

    const numbers = fn.descendantsOfType("number_literal");
    for (const numberNode of numbers) {
      if (isAllowedNumericLiteral(numberNode)) {
        continue;
      }

      diagnostics.push({
        ruleId: "no-magic-number",
        message: `Extract magic number ${numberNode.text} in ${functionName.text}`,
        start: numberNode.startIndex,
        end: numberNode.endIndex,
      });
    }
  }

  return diagnostics;
}

export default {
  register() {
    return {
      name: "starter-plugin",
      description: "Starter plugin with practical runtime hygiene rules.",
      rules: {
        "no-debug-print": {
          code: "JSP001",
          title: "avoid debug.print in runtime entrypoints",
          description:
            "debug.print can leak internals and should not stay in runtime entrypoint code.",
          help: "remove debug.print or guard it behind explicit debug-only control flow.",
          severity: "warning",
          docsUrl: "https://example.com/docs/jsp001",
        },
        "no-magic-number": {
          code: "JSP002",
          title: "avoid magic numbers in runtime entrypoints",
          description:
            "Hardcoded numeric literals in runtime logic make behavior harder to audit and maintain.",
          help: "Extract the value into a named constant and reference it from runtime logic.",
          severity: "warning",
          docsUrl: "https://example.com/docs/jsp002",
        },
      },
    };
  },
  lint,
};
