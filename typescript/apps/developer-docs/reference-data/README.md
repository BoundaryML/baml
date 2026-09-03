# Generated reference data

These immutable release datasets come from `baml describe <package> --export` and are rendered by the Fumadocs application under `/baml/packages/<selector>/<package>/<object...>`. Do not hand-edit them.

Each dataset retains the complete producer export under `describe`. The compact
`catalog` is only a stable route/search index; renderers must not treat it as a
lossless replacement for compiler metadata.

The toolchain selector is a generated-reference concern. It belongs on BAML
package pages and, once its structured dataset is connected, generated CLI
pages. It must not become a global docs selector: the book and authored
language reference stay pinned through fixture `// TRACK` metadata and render
regions extracted with `// ANCHOR` / `// ANCHOR_END`.

Regenerate the current exact release and its `latest` channel pointer with:

```sh
pnpm --filter @baml/developer-docs generate:reference
```

`channels.v1.json` resolves mutable selectors such as `latest` to an exact release directory. Each release envelope records its dataset schema, describe-format version, release track, toolchain version, BAML-language source revision, and SHA-256 digest of the complete producer output. Git stores the narrow vertical-slice seed; a later ingestion service can retain all packages and releases without changing public package routes.
