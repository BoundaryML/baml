# BAML TextMate grammar

The TextMate grammar for [BAML](https://github.com/BoundaryML/baml) (`scopeName: source.baml`), published as [`@boundaryml/baml-grammar`](https://www.npmjs.com/package/@boundaryml/baml-grammar).

This repository is a read-only mirror, generated from [`typescript2/pkg-grammar`](https://github.com/BoundaryML/baml/tree/canary/typescript2/pkg-grammar) in the BAML monorepo. Do not edit it by hand; changes land here automatically when the grammar changes upstream. It is also the source [GitHub Linguist](https://github.com/github-linguist/linguist) vendors for `.baml` highlighting on github.com.

## Install

```sh
npm install @boundaryml/baml-grammar
```

## Shiki

The default export is a ready-to-use `LanguageRegistration`:

```ts
import { createHighlighter } from "shiki";
import baml from "@boundaryml/baml-grammar";

const highlighter = await createHighlighter({
  themes: ["github-dark"],
  langs: [baml],
});

highlighter.codeToHtml(code, { lang: "baml", theme: "github-dark" });
```

The grammar is self-contained (it embeds no other language scopes), so no additional grammars need to be loaded.

## VS Code / vscode-textmate / Monaco (monaco-textmate)

Use the raw grammar JSON, plus the editor configuration (brackets, comments, auto-closing pairs):

```ts
import grammar from "@boundaryml/baml-grammar/baml.tmLanguage.json" with { type: "json" };
import languageConfiguration from "@boundaryml/baml-grammar/language-configuration.json" with { type: "json" };
```

If your bundler cannot handle JSON import attributes (Metro/React Native, some Vite configs), use the default export instead: it is the same grammar inlined in a plain JS module.

```ts
import grammar from "@boundaryml/baml-grammar";
```

## Files

| Path | Contents |
| --- | --- |
| `dist/index.js` | ESM module, default-exports the grammar (Shiki `LanguageRegistration`) |
| `grammars/baml.tmLanguage.json` | Raw TextMate grammar |
| `grammars/baml.sublime-syntax` | Sublime Text syntax (also consumed by syntect/`bat`) |
| `syntaxes/baml.xml` | KDE KSyntaxHighlighting definition (Kate, Pandoc via skylighting) |
| `samples/baml.sample` | Canonical BAML sample used by grammar registries |
| `language-configuration.json` | VS Code style language configuration |

The paths above (and `scopeName: source.baml`) are frozen API: external registries fetch them by URL. Renames are breaking changes. `SUPPORT.md` tracks where the grammar has landed.

## License

Apache-2.0

---

This repository is a **read-only mirror**, generated from [`typescript2/pkg-grammar`](https://github.com/BoundaryML/baml/tree/canary/typescript2/pkg-grammar) in the BAML monorepo by its `sync-grammar-mirror` workflow. Do not edit files or open pull requests here — changes land automatically when the grammar changes upstream. Report issues in [BoundaryML/baml](https://github.com/BoundaryML/baml/issues).
