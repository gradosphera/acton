# JS Plugin Starter

Minimal Acton project with a JavaScript lint plugin that receives `source`, raw `cst`, and a tree-sitter-style API at `tree.rootNode`.

## Structure

- `Acton.toml` - enables JS plugin via `[lint].js-plugins`
- `contracts/main.tolk` - sample contract
- `plugins/simple-plugin.mjs` - sample plugin using `descendantsOfType` and `childForFieldName`
- `plugins/acton-plugin-api.d.ts` - local typings for IDE autocompletion in JS plugins

For top-level `fun` declarations, use kind `function_declaration` (from `crates/tree-sitter-tolk/grammar.js`).

## Run

```bash
cd examples/js-plugin-starter
acton check
```

You should see a warning from `E026` with message from `simple-plugin.mjs`.
