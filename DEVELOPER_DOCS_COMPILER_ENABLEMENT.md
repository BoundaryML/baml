# Supplement: Minimal Compiler and CLI Enablement for Developer Documentation

## Status and relationship to the main proposal

This document defines the minimum compiler-, CLI-, and release-facing work needed by [DEVELOPER_DOCS_PROPOSAL.md](./DEVELOPER_DOCS_PROPOSAL.md).

The conclusion for the initial portal is deliberately small:

> The developer-documentation work has no compiler blocker and requires no new public compiler API, CLI command, or diagnostic format.

The main proposal remains authoritative for routes, content ownership, versioning, and portal behavior. This supplement identifies the existing interfaces the portal can consume, the private adapter scripts it needs, and the improvements that are explicitly deferred.

The existing repository-root `docs/` and `fern/` directories are not inputs. They must never be read, imported, copied, or used to infer behavior.

## Required versus optional

### Required for the initial portal

1. Use the existing versioned standard-package export for a checked-in documentation publication allowlist.
2. Generate package and CLI artifacts for an exact released toolchain version.
3. Store those derived artifacts in PlanetScale Postgres under immutable exact-version keys.
4. Validate BAML documentation snippets with the existing `baml check` command.
5. Keep all documentation-specific parsing, temporary-file handling, assertions, and artifact shaping in private scripts owned by the documentation application or release workflow.
6. Fail generation on malformed data, duplicate fully qualified routes, unknown or unbalanced embedded snippet metadata, or mismatched expected diagnostics.

### Not required for the initial portal

- A public `baml docs` or `baml export-docs` command
- `baml check --file`
- A JSON diagnostic mode
- A documentation-specific compiler SDK or `docs_snippet_check` API
- A new stable CLI metadata DTO
- A typed wrapper-command specification
- Typed configuration, environment-variable, or exit-code catalogs
- A second standard-package export schema
- Changes to BAML parsing, type checking, or project discovery
- Execution of CLI examples
- Compilation or execution of bridge examples

These may become worthwhile later, but none should be added preemptively.

## Existing interfaces

### Standard packages

The public CLI already exports a complete package surface as versioned JSON:

```text
baml describe <package> --export
```

The implementation is backed by `baml_ide::PackageExport` and `baml_ide::export_package` in:

```text
baml_language/crates/baml_ide/src/export.rs
```

The export is already JSON-serializable, deterministically ordered, schema-versioned, snapshot-tested, checked for duplicate IDs, and used by another repository consumer. The documentation pipeline should consume this format unchanged rather than inventing a parallel compiler model.

The confirmed initial set published by the portal is a deliberately checked-in documentation allowlist:

```yaml
# typescript2/app-developer-docs/content-data/reference/stdlib-packages.yaml
packages:
  - baml
  - reflect
  - boundary
  - testing
  - assert
  - log
  - ai
  - openai
  - anthropic
  - google
  - aws
  - vercel
  - claude_code
```

Standard-library packages are introduced rarely, so this small explicit list is preferable to adding and maintaining compiler-side discovery infrastructure. It is the portal's publication selection, not a claim that the compiler lacks other internal packages. Introducing or retiring a public package must update this allowlist in the same change.

The private documentation generator runs `baml describe <package> --export` for every allowlisted package using the exact released toolchain. A listed package that cannot be exported fails generation. A compiler package that is not allowlisted is not published automatically.

The documentation adapter projects pages only for packages, namespaces, and top-level classes, enums, interfaces, aliases, and functions. It converts their fully qualified names directly into URL segments and checks uniqueness across the release. A collision fails generation; it must not be resolved by inserting a `[kind]` segment.

Fields, methods, variants, associated types, and implementation details stay inside their owning page data. Search and cross-references link them through deterministic anchors rather than independent path routes. Every exported nested docstring must be preserved in that page data.

### CLI reference

The installed exact-version executable already exposes its public command documentation through:

```text
baml help
baml help <command> [<subcommand> ...]
baml toolchain help
baml self-update --help
```

For the initial portal, a private release script must recursively capture and minimally parse this plain, noncolored help output. The resulting artifact is derived data for that exact binary version. It is not a new CLI contract.

The parser should extract only facts that are unambiguous in emitted help, such as command paths, usage, descriptions, visible arguments, flags, defaults, allowed values, and examples. It must retain the original help text in the generated artifact and fail loudly when an expected section can no longer be parsed. It must not guess missing facts.

The wrapper currently owns command roots that are not discoverable from the toolchain's command tree. The initial adapter must explicitly invoke the known wrapper help entry points above. This small discovery list is adapter configuration, not duplicated documentation: the captured executable output remains the source rendered by the portal. A newly added wrapper command will require updating the adapter until the wrapper eventually offers a discoverable command model.

Complete machine-readable configuration, environment-variable, and exit-code catalogs are not prerequisites for the architecture proof. Initial exact-version pages may contain only facts that can be derived reliably from the released command surface plus clearly separated authored guidance. If these references later need exhaustive runtime metadata, that is when typed internal catalogs should be proposed.

### Release identity

The selected compiled `baml` binary is authoritative for the canonical product and wrapper versions used by a publication run. Production CI receives the release time, source commit, and optional channel from the release workflow. An operator-run publication supplies the full source commit and release time explicitly unless the binary exposes trustworthy equivalent metadata; it may supply an optional channel pointer to move after publication. The documentation tooling supplies its own generator version, and `generated_at` records the execution time. Neither workflow infers release identity from a branch name or substitutes the current clock for the release time.

The existing BAML TextMate grammar under `typescript2/pkg-grammar` is also sufficient for syntax highlighting. It requires no compiler work.

## Private documentation adapters

The private adapters live with the documentation application:

```text
typescript2/app-developer-docs/
├── content-data/reference/
│   └── stdlib-packages.yaml
├── scripts/
│   ├── generate-package-reference.ts
│   ├── generate-cli-reference.ts
│   ├── populate-generated-content.ts
│   ├── migrate-generated-content.ts
│   ├── inspect-generated-content.ts
│   ├── verify-generated-content.ts
│   └── validate-baml-snippets.ts
└── lib/snippets/
    ├── metadata.ts
    └── regions.ts
```

The package generator does only the following:

1. Read `content-data/reference/stdlib-packages.yaml`.
2. Invoke `baml describe <package> --export` for each entry using the exact release binary.
3. Preserve the exact canonical JSON text and hash.
4. Project package, namespace, and top-level declaration pages under the current `PAGE_SCHEMA_VERSION`.
5. Preserve all exported nested members, implementations, and docstrings in their owning page data.
6. Validate qualified-name, route, and anchor uniqueness.
7. Return an in-memory package publication input to the shared validator and publisher.

These scripts must not introduce a new public command, a second semantic model, or documentation-specific compiler behavior. Their parsing and output formats are portal implementation details.

The CLI generator likewise returns one in-memory CLI publication input. Neither generator writes to the database independently. The shared population command combines both inputs, validates the complete exact release, and performs the single PlanetScale transaction defined in the main proposal. The same population implementation is used by operator-run and CI/CD workflows.

## Snippet extraction and validation

### Source and expectation model

Canonical snippet source remains under:

```text
typescript2/app-developer-docs/content/code/
```

There is no external snippet manifest. A standalone snippet ID is its POSIX-style path relative to `content/code/standalone/`, without `.baml`. A project ID is its directory path relative to `content/code/projects/`. For example:

```text
content/code/standalone/invalid-return-type.baml
→ <BamlSnippet id="invalid-return-type" />

content/code/projects/cross-file-example/
→ <BamlProject id="cross-file-example" />
```

Nested examples retain their relative path in the ID. Duplicate derived IDs within a source type fail validation. Standalone and project IDs are resolved by different components and therefore occupy separate namespaces.

The region extractor recognizes documentation comments such as:

```baml
// docs:start example
function ClassifyMessage(message: string) -> string {
  return message
}
// docs:end example
```

This is a marker parser, not a BAML parser. It should only locate balanced, uniquely named regions. The compiler remains responsible for understanding BAML.

Expected outcomes are embedded as a YAML block inside ordinary BAML comments:

```baml
// docs:meta
// expect:
//   status: failure
//   diagnostics:
//     - code: E0001
//       messageContains: "expected string"
// docs:endmeta

// docs:start example
function InvalidReturnType() -> string {
  return 42
}
// docs:end example
```

The parser strips exactly one ordinary `//` comment prefix from each metadata line and parses the remainder as YAML. Plain comments are used so the metadata does not become documentation attached to a BAML declaration.

Success is the default, so valid examples usually need only region markers. A standalone file may contain at most one metadata block. A project may contain at most one metadata block across all files in `baml_src/`; its expectation applies to the complete project.

The parser must reject unknown metadata fields, malformed YAML, multiple metadata blocks for one source, contradictory expectations, missing or duplicate regions, path traversal, duplicate derived IDs, and project fixtures without both `baml.toml` and `baml_src/`.

### Standalone files

Current `baml check --project <directory>` accepts a manifest-less directory and recursively checks the `.baml` files in it. Passing a file path is not isolated because project discovery can include sibling files.

The validator therefore creates a fresh temporary directory for every standalone source, copies only that complete `.baml` file into it, and runs:

```text
baml \
  --output-preset agent \
  --color never \
  --no-progress \
  --diagnostic-format agent \
  check \
  --project <temporary-directory>
```

Use the operating system's temporary-directory API, for example Node's `os.tmpdir()` plus `fs.mkdtemp()`, rather than hard-coding `/tmp`. Cleanup belongs in a `finally` block. Each invocation receives a fresh directory so one snippet cannot see another snippet's files.

The complete canonical file is compiled even when the rendered page displays only one marked region.

### Proper multi-file projects

A multi-file fixture is always a normal BAML project containing:

```text
baml.toml
baml_src/
```

It is checked in place with:

```text
baml \
  --output-preset agent \
  --color never \
  --no-progress \
  --diagnostic-format agent \
  check \
  --project content/code/projects/cross-file-example
```

The validator does not synthesize a manifest, rearrange project files, or support arbitrary multi-file directories.

### Expected success and failure

The validator uses the process exit status as the primary success signal:

- `expect.status: success` requires exit status zero.
- `expect.status: failure` requires a nonzero exit status.
- An expected failure must also match every declared diagnostic code.
- `messageContains` is an optional secondary assertion.

The existing agent diagnostic output contains stable diagnostic codes such as `E0001`. The documentation script may extract those codes and nearby message text with a deliberately small parser. If the output can no longer be understood, validation fails with the raw captured output; that parser failure does not justify adding a compiler API in advance.

Full diagnostic prose and source spans should not be snapshotted by the documentation system.

## Generated artifact contract

The authoritative PlanetScale DDL and publication transaction are defined once in the main proposal's **PlanetScale schema** section. This supplement does not introduce a competing envelope or storage layout.

The five tables are:

```text
developer_docs.releases
developer_docs.channel_pointers
developer_docs.package_exports
developer_docs.reference_pages
developer_docs.cli_artifacts
```

The compiler-facing rules are:

- `package_exports.describe_output_json` preserves the exact canonical output of `baml describe <package> --export` as text.
- `package_exports.describe_format_version` remains the compiler-owned `PackageExport::FORMAT_VERSION`.
- `reference_pages.page_schema_version` belongs to the documentation projection and is manually bumped when projection output materially changes.
- Reference pages are created only for packages, namespaces, and top-level declarations. Members and impls remain nested in `page_data` with every available docstring.
- `cli_artifacts.payload_json` contains the parsed command tree and raw help by command. There is no CLI page-projection table.
- Exact releases and their authoritative artifacts are immutable; channel pointers are mutable.
- There is no building, published, or failed status. A release row exists only as part of a complete committed publication transaction.
- Initial population resolves canary and nightly to exact canonical versions, inserts complete data for both, and then creates the two pointers without labeling either stable.
- A retry may proceed only when existing authoritative hashes and projection rows are identical.

PlanetScale credentials are supplied only through a deployment secret such as `GENERATED_CONTENT_DATABASE_URL`; they must never be written into this document, repository files, logs, client bundles, or generated artifacts.

## Minimal work breakdown

### Compiler and public CLI

No required changes.

Existing package export, `baml check`, diagnostic codes, agent output, help commands, release versioning, and grammar are consumed as they are.

### Private release tooling

1. Add the allowlist-driven package export and page-projection script.
2. Add the CLI help capture/parser that emits one canonical CLI artifact.
3. Add one operator-run population command that invokes a selected compiled `baml` binary and can perform a write-free dry run.
4. Generate and validate all exact-version data in memory before database publication.
5. Insert the release, exports, projections, CLI artifact, and optional channel pointer in one PlanetScale transaction.

### Developer portal

1. Add strict embedded-metadata and region-marker parsers.
2. Add isolated temporary-directory validation for standalone files.
3. Validate proper projects directly.
4. Check success/failure expectations and diagnostic codes.
5. Fetch and verify generated reference artifacts during the portal build.

## Acceptance criteria

The initial portal is not blocked on compiler enablement when all of the following are true:

- No new public documentation command or compiler API has been added.
- No `baml check --file` or JSON diagnostic mode has been added.
- Every package in `content-data/reference/stdlib-packages.yaml` is exported through the exact released `baml describe <package> --export` command.
- Adding or retiring a public standard package updates the publication allowlist in the same change.
- Generated package, namespace, and top-level declaration routes use fully qualified names without `[kind]` and fail on collisions.
- Nested members and implementation details stay in owning page data, retain exported docstrings, and receive deterministic anchors rather than independent routes.
- The exact-version CLI artifact is captured from the corresponding released executable.
- CLI routes are rendered directly from that artifact without a CLI page-projection table.
- Generated artifacts contain release provenance and are stored immutably in PlanetScale outside Git.
- PlanetScale uses the five tables and constraints defined in the main proposal, with no publication-status or parent columns.
- The initial PlanetScale population contains exact package and CLI snapshots for the current canary and nightly versions plus pointers for both channels.
- A valid standalone file succeeds from its isolated temporary directory.
- An invalid standalone file fails with its declared diagnostic code.
- A proper project containing `baml.toml` and `baml_src/` compiles using the normal project path.
- Snippet IDs, expectations, source, and displayed regions derive from the same canonical files or project directories.
- CLI and bridge examples are not executed by this validation system.
- No repository-root `docs/` or `fern/` content participates in the pipeline.

## Deferred improvements and their trigger

| Improvement | Add it only when |
|---|---|
| `baml check --file` | Another real user workflow needs isolated single-file checking and temporary directories have proved inadequate. |
| Structured diagnostic JSON | Multiple external consumers need a stable diagnostic protocol that justifies long-term compatibility guarantees. |
| Stable CLI metadata DTO | Parsing emitted help repeatedly blocks required documentation fidelity or reliability. |
| Typed wrapper command model | Wrapper commands expand enough that the explicit capture entry points become unsafe or burdensome. |
| Typed configuration/environment/exit catalogs | Those exact-version reference pages require exhaustive facts that current canonical outputs cannot provide. |
| Additional nested docstring fields | The compiler's semantic model retains useful associated-type, generic-parameter, function-parameter, or associated-binding documentation that the current export omits. |
| Automatic standard-package discovery | Package churn makes the explicit publication allowlist materially error-prone. |
| Independent wrapper-doc version axis | Wrapper behavior changes frequently enough that recording its version inside a toolchain artifact is insufficient. |

Each deferred item requires its own proposal. None is implicitly authorized by the developer-portal implementation.

## Decisions

| Question | Decision | Reason |
|---|---|---|
| Is compiler work blocking the portal? | No | Existing commands and exports are sufficient when wrapped by private scripts. |
| Add `baml docs export`? | No | Documentation publishing is not a public user workflow. |
| Add `baml check --file`? | No | An isolated temporary directory gives the existing project command exactly one file. |
| Add JSON diagnostics? | No | Exit status plus existing diagnostic codes is sufficient for documentation CI. |
| Parse BAML in the docs app? | No | The docs parser recognizes only comment-delimited YAML metadata and region markers; the compiler parses BAML. |
| How are multi-file examples represented? | Proper projects | Every such fixture contains `baml.toml` and `baml_src/`. |
| How are packages selected? | Checked-in publication allowlist | New public packages are rare, and an explicit list avoids compiler-side discovery infrastructure. |
| Where do snippet IDs and expectations live? | In their paths and `.baml` comments | This removes the external snippet manifest and its repeated ID and source path. |
| Add `[kind]` to package routes? | No | FQNs are canonical; route collisions fail generation. |
| Give members independent path routes? | No | Members and impls remain in owning page data and use deterministic anchors. |
| Store page parents? | No | Parent hierarchy is derived from required dotted qualified names. |
| Store release publication status? | No | The complete release is inserted atomically; no row means unpublished. |
| How is CLI metadata obtained initially? | Capture exact-binary help | It avoids a new maintained API and preserves the actual released command surface. |
| Project CLI pages into separate rows? | No | The portal build renders directly from the smaller exact-version CLI artifact. |
| Can package or CLI generators publish independently? | No | They return in-memory inputs to one shared validator and atomic publisher. |
| Use separate manual and CI/CD publication implementations? | No | The same population implementation serves both workflows. |
| Where is generated metadata stored? | PlanetScale Postgres | Exact-version derived records remain outside Git and are retrieved at portal build time. |
| What is populated initially? | Current canary and nightly snapshots | Both resolve to immutable exact versions; mutable channel pointers reference them without calling either stable. |
