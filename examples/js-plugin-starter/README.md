# JS Plugin Starter

Minimal Acton project with a JavaScript lint plugin that receives `source`, raw `cst`, and a tree-sitter-style API at `tree.rootNode`.

## Structure

- `Acton.toml` - enables JS plugin via `[lint].js-plugins`
- `contracts/main.tolk` - sample contract
- `plugins/simple-plugin.mjs` - practical plugin using `register()` + `lint()`
- `plugins/acton-plugin-api.d.ts` - local typings for IDE autocompletion in JS plugins

For top-level `fun` declarations, use kind `function_declaration` (from `crates/tree-sitter-tolk/grammar.js`).
The sample plugin implements two real rules:
- `no-debug-print` (`JSP001`) - reports `debug.print(...)` in runtime entrypoints.
- `no-magic-number` (`JSP002`) - reports hardcoded numeric literals (except `0` and `1`) in runtime entrypoints.

Runtime entrypoints are `main`, `onInternalMessage`, and `onExternalMessage`.
The plugin registers rule metadata (`code`, `title`, `description`, `help`, `severity`) and returns diagnostics via `ruleId`.

## Run

```bash
cd examples/js-plugin-starter
acton check
```

You should see warnings with codes `JSP001` and `JSP002` from `simple-plugin.mjs`.
