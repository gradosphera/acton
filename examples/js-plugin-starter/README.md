# JS Plugin Starter

Minimal Acton project with a JavaScript lint plugin that receives `source`, raw `cst`, and a tree-sitter-style API at `tree.rootNode`.

## Structure

- `Acton.toml` - enables JS plugin via `[lint].js-plugins`
- `contracts/main.tolk` - sample contract
- `plugins/simple-plugin.mjs` - sample plugin using `register()` + `lint()`
- `plugins/acton-plugin-api.d.ts` - local typings for IDE autocompletion in JS plugins

For top-level `fun` declarations, use kind `function_declaration` (from `crates/tree-sitter-tolk/grammar.js`).
The sample plugin registers rule metadata (`code`, `title`, `description`, `help`, `severity`) and then returns diagnostics via `ruleId`.
It also demonstrates how to find variable `profile` by name and read the inferred type of its struct literal via `ctx.typeOf(node)` / `node.inferredType`.

## Run

```bash
cd examples/js-plugin-starter
acton check
```

You should see a warning from `E026` with message from `simple-plugin.mjs`.
