# Where BAML syntax highlighting has landed

Status of the BAML grammar across syntax-highlighting registries. The grammar
is maintained in [`typescript2/pkg-grammar`](https://github.com/BoundaryML/baml/tree/canary/typescript2/pkg-grammar)
(and sibling `pkg-grammar-*` packages) in the BAML monorepo; this file is
updated there.

| Registry | Reaches | Format | Status |
| --- | --- | --- | --- |
| [GitHub Linguist](https://github.com/github-linguist/linguist) | github.com, starry-night | TextMate (this repo) | shipped |
| npm [`@boundaryml/baml-grammar`](https://www.npmjs.com/package/@boundaryml/baml-grammar) | Shiki custom langs, vscode-textmate, Monaco | TextMate (this repo) | shipped |
| [Shiki](https://github.com/shikijs/textmate-grammars-themes) | VitePress, Astro, Nextra, rehype-pretty-code | TextMate (this repo) | planned |
| VS Code Marketplace | VS Code, Cursor, vscode.dev | TextMate | shipped (BAML extension) |
| [bat](https://github.com/sharkdp/bat) / syntect | bat, delta, Zola | `grammars/baml.sublime-syntax` (this repo) | planned |
| Sublime Text Package Control | Sublime Text | `grammars/baml.sublime-syntax` (this repo) | planned |
| [KSyntaxHighlighting](https://invent.kde.org/frameworks/syntax-highlighting) | Kate, KDE, Pandoc (skylighting) | `syntaxes/baml.xml` (this repo) | planned |
| npm `@boundaryml/baml-highlightjs` | highlight.js sites (mdBook, Discourse, blogs) | [BoundaryML/baml-highlightjs](https://github.com/BoundaryML/baml-highlightjs) | planned |
| [BoundaryML/baml-treesitter](https://github.com/BoundaryML/baml-treesitter) | Neovim, Zed, Helix, Emacs 29+ | tree-sitter | planned |
| [Pygments](https://github.com/pygments/pygments) | Sphinx, MkDocs, Jupyter, minted | hand-written lexer | planned |
| [Rouge](https://github.com/rouge-ruby/rouge) | GitLab, Jekyll | hand-written lexer | planned |
| [Chroma](https://github.com/alecthomas/chroma) | Hugo, Gitea/Forgejo | XML (from Pygments lexer) | planned |
