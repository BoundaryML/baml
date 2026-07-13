# BAML language for highlight.js

A [highlight.js](https://highlightjs.org) third-party language definition for [BAML](https://github.com/BoundaryML/baml), published as [`@boundaryml/baml-highlightjs`](https://www.npmjs.com/package/@boundaryml/baml-highlightjs).

It highlights the full BAML surface: declaration blocks (`class`, `enum`, `interface`, `function`, `client<llm>`, `generator`, `retry_policy`, `template_string`, `test`, `testset`), expression bodies (`let` / `const` / `if` / `match` / `for` / `while` / `spawn` / `defer` / `watch`), attributes (`@alias(...)`, `@@dynamic`), `env.VAR` references, and all BAML string forms — including Jinja `{{ ... }}` / `{% ... %}` markup inside `#"..."#` prompt bodies and `${ ... }` interpolation inside backtick strings.

## Install

```sh
npm install highlight.js @boundaryml/baml-highlightjs
```

`highlight.js` (v11) is a peer dependency.

## Usage

```js
import hljs from "highlight.js/lib/core"; // or "highlight.js" for the full build
import baml from "@boundaryml/baml-highlightjs";

hljs.registerLanguage("baml", baml);

const { value } = hljs.highlight(code, { language: "baml" });
```

Code fences tagged `baml` then highlight with any hljs-based renderer (markdown-it, marked-highlight, rehype-highlight, ...), as long as the language is registered first.

## CDN

The package ships a plain ES module, so it works directly from an ESM CDN — no build step:

```html
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/highlight.js@11/styles/github-dark.min.css" />
<script type="module">
  import hljs from "https://cdn.jsdelivr.net/npm/highlight.js@11/+esm";
  import baml from "https://cdn.jsdelivr.net/npm/@boundaryml/baml-highlightjs/+esm";

  hljs.registerLanguage("baml", baml);
  hljs.highlightAll();
</script>
```

## Browser / bundled builds

For the classic (non-module) highlight.js build, `dist/baml.js` self-registers the language on the global `hljs` — load it after highlight.js:

```html
<script src="https://cdn.jsdelivr.net/npm/@highlightjs/cdn-assets@11/highlight.min.js"></script>
<script src="https://cdn.jsdelivr.net/npm/@boundaryml/baml-highlightjs/dist/baml.js"></script>
```

Bundler users should import the ES module (`src/baml.js`, the package default export) instead.

## Files

| Path | Contents |
| --- | --- |
| `src/baml.js` | ESM module, default-exports the highlight.js `LanguageFn` |
| `dist/baml.js` | Classic-script build; self-registers `baml` on the global `hljs` |

## License

Apache-2.0

---

This repository is a **read-only mirror**, generated from [`typescript2/pkg-grammar-hljs`](https://github.com/BoundaryML/baml/tree/canary/typescript2/pkg-grammar-hljs) in the BAML monorepo. Do not edit it by hand; changes land here automatically when the language definition changes upstream.
