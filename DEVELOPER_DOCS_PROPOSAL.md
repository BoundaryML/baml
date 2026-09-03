# Proposal: Boundary Developer Documentation

## Status

This document records the current architecture and content decisions for a new Boundary developer documentation portal. It is intended to constrain the initial implementation and prevent unresolved details from being decided implicitly during scaffolding.

The minimal compiler, CLI, private-adapter, and release integration is specified in [DEVELOPER_DOCS_COMPILER_ENABLEMENT.md](./DEVELOPER_DOCS_COMPILER_ENABLEMENT.md). That supplement concludes that the initial portal requires no new public compiler API, CLI command, or diagnostic format.

## Context

Boundary needs a technical documentation platform for three related product surfaces:

- **BAML** — the language, book, language reference, standard packages, and language bridges.
- **BAML CLI** — installation, commands, configuration, and local development workflows.
- **Boundary Cloud Services (BCS)** — a future cloud product surface that is still being defined.

We reviewed how mature developer ecosystems organize learning material, product documentation, and reference content:

- HashiCorp gives related products distinct sections within one developer portal.
- NVIDIA treats the CUDA Programming Guide as a first-class publication alongside compiler, library, debugging, and API reference.
- Apple keeps most public technical content under one developer domain and separates documentation, tutorials, samples, and support through paths.

The useful pattern is one recognizable developer destination with clear product boundaries. Boundary is small enough to implement this as one application and one deployment pipeline rather than a collection of separately operated documentation sites.

Boundary's requirements also go beyond ordinary Markdown:

- BAML syntax highlighting must evolve with the language.
- Standard-package and CLI reference should be generated from canonical implementation sources.
- BAML examples must be continuously compiled so documentation cannot silently become stale.
- Documentation examples should eventually be editable and runnable through BAML WASM.
- The BAML book needs interactive components such as quizzes.
- Authored and generated content need to coexist in one navigation and search system.
- The platform should leave room for richer language tooling later.

These requirements affect the rendering pipeline, build process, release model, and component system. They are not isolated customizations around an otherwise conventional documentation site.

The proposal is therefore to build one developer portal using Next.js and Fumadocs. Boundary owns the application without building the underlying documentation framework from scratch.

## Public developer portal

The portal will live at:

```text
developer.boundaryml.com
```

The initial route structure is:

```text
/
├── baml/
│   ├── get-started/
│   ├── book/
│   ├── language/
│   ├── packages/
│   │   └── [version]/
│   └── bridges/
│       └── [language]/
│
├── cli/
│   └── [version]/
│       ├── installation/
│       ├── commands/
│       ├── configuration/
│       ├── environment-variables/
│       └── exit-codes/
│
├── bcs/
├── tutorials/
├── examples/
└── changelog
```

The top-level organization is intentionally product-first:

- BAML contains language learning and reference.
- BAML CLI contains toolchain-specific documentation.
- Boundary Cloud Services reserves a place for the future cloud product.
- Tutorials and examples remain top-level because realistic workflows can cross product boundaries.

There is no generic top-level `/reference` section. Reference material belongs to the product that defines it.

There is no integrations section because Boundary does not currently have an integration catalog to document. It can be introduced later if it becomes a real product or documentation surface.

Every recognized section root must render a useful landing page, catalog, or table of contents. Section roots must not automatically forward to their first child or to a floating "latest" URL.

### Route behavior

The route contract is explicit so the implementation does not invent redirect or fallback behavior:

| Route | Expected behavior |
|---|---|
| `/` | Portal landing page summarizing BAML, the CLI, BCS, tutorials, examples, and the changelog. |
| `/baml` | BAML overview with clear entry points into getting started, the book, language reference, packages, and bridges. |
| `/baml/vMAJOR.MINOR.PATCH` | Contextual 404. The BAML language guide is not versioned as a whole. |
| `/baml/get-started` | Authored getting-started page. |
| `/baml/book` | Book introduction and table of contents. |
| `/baml/book/[part]` | Part summary only when the book's authored structure defines one; otherwise no route is created. |
| `/baml/book/[part]/[chapter]` | Authored book chapter. |
| `/baml/language` | Language-reference overview and table of contents. |
| `/baml/language/[topic]` | Authored topic after the language taxonomy is defined. |
| `/baml/packages` | Generated package catalog showing the latest stable release when one is published; otherwise the available canary and nightly snapshots. It also shows allowlisted packages, all published versions, release dates or channels, availability and deprecation metadata, and changelog links. |
| `/baml/packages/[version]` | Exact-version package index. |
| `/baml/packages/[version]/[...FQN]` | Generated package, namespace, or top-level declaration page addressed by the remaining path segments of its fully qualified BAML name. There is no definition-kind segment. Members remain on their owning declaration page. |
| `/baml/bridges` | Bridge overview and cross-language compatibility summary. |
| `/baml/bridges/[language]` | Canonical compatibility, type-transition, rules, and gotchas page for one host language. |
| Deeper `/baml/bridges/...` paths | 404 until the content demonstrates a real need for subpages. |
| `/cli` | CLI overview showing the latest stable release when one is published; otherwise the available canary and nightly snapshots. It also shows installation entry points, the command catalog, all published versions, and release metadata. |
| `/cli/[version]` | Exact-version CLI overview. |
| `/cli/[version]/installation` | Exact-version installation methods and supported environments. |
| `/cli/[version]/commands` | Exact-version command index. |
| `/cli/[version]/commands/[command]/[subcommand]` | Generated command or subcommand page; nesting mirrors actual CLI tokens. |
| `/cli/[version]/configuration` | Exact-version configuration reference. |
| `/cli/[version]/environment-variables` | Exact-version environment-variable reference. |
| `/cli/[version]/exit-codes` | Exact-version exit-code reference. |
| `/bcs` | Boundary Cloud Services coming-soon landing page. |
| Deeper `/bcs/...` paths | 404 until an actual product and API information architecture exists. |
| `/tutorials` and `/examples` | Authored indexes. |
| `/tutorials/[tutorial]` and `/examples/[example]` | Authored leaf pages; routes exist only for authored entries. |
| `/changelog` | Canonical changelog rendered directly from `baml_language/CHANGELOG.md`. |

Exact version segments use `v` followed by the canonical BAML version string. Stable versions look like `vMAJOR.MINOR.PATCH`; canary or nightly snapshots may include the canonical prerelease identifier. The channel names `canary` and `nightly` are mutable pointers and are not themselves exact-version route segments. Unknown versions, definitions, commands, and authored slugs return a real 404. A contextual 404 may link to the parent index, current stable release when one exists, available channel snapshots, search, or the changelog, but it remains a 404 and must never silently substitute another page.

Authored route segments use lowercase kebab-case. Generated fully qualified names preserve their canonical spelling. Canonical URLs omit a trailing slash.

## BAML

The BAML section represents the language:

```text
/baml
```

It contains the shortest getting-started path, the book, language documentation, generated standard-package reference, and language bridges.

### Getting started

```text
/baml/get-started
```

The getting-started guide is the shortest route from installation to running a first BAML function.

It is separate from the book because the two experiences serve different purposes:

- Getting started optimizes for immediate success.
- The book teaches the language systematically.
- The language section helps developers who already know what they are looking for.

### The BAML book

```text
/baml/book
/baml/book/[part]/[chapter]
```

The book is a first-class publication inside the BAML section rather than a separate site or domain.

Its source remains Markdown or MDX. Book-specific behavior becomes normal portal functionality:

- BAML listings are validated in CI.
- Quizzes become React components.
- A later enhancement can add editable and runnable examples through shared `BamlSnippet` and `BamlProject` behavior.
- Syntax highlighting comes from the canonical BAML grammar.
- Chapter ordering and previous/next navigation come from the content structure.

Keeping the book in the portal allows it to share navigation, search, previews, syntax highlighting, and runnable infrastructure with the rest of the documentation.

### Language documentation

```text
/baml/language
```

The language section covers syntax, types, declarations, expressions, control flow, attributes, diagnostics, and other language concepts.

The exact subpage structure is intentionally deferred. It should emerge from the language's actual conceptual model rather than being fixed prematurely.

Language documentation can be hybrid:

- The compiler supplies current facts such as keywords, operators, built-in types, and constraints.
- Humans supply explanations, examples, comparisons, rationale, and guidance.

The language section documents the current supported BAML language. It does not receive a historical version selector. Historical differences belong in the changelog and migration material.

### Standard packages

The BAML standard library is presented as a collection of packages:

```text
/baml/packages
/baml/packages/[version]
```

Example package roots include:

```text
/baml/packages/v0.18.0/baml
/baml/packages/v0.18.0/assert
/baml/packages/v0.18.0/reflect
/baml/packages/v0.18.0/testing
```

Packages, namespaces, and top-level declarations are addressed using their fully qualified BAML names:

```text
/baml/packages/v0.18.0/baml/json/parse
/baml/packages/v0.18.0/baml/String
/baml/packages/v0.18.0/reflect/Type
```

A separate definition-kind segment is unnecessary because fully qualified BAML names are unique.

The route generator must preserve canonical identifier spelling and validate route uniqueness. A collision should fail generation rather than produce an ambiguous page. The stored `page_kind` is rendering metadata only and never contributes a path segment.

Fields, methods, enum variants, associated types, required and default interface methods, implementation blocks, and implementation methods remain nested in their owning class, enum, or interface page. They do not receive independent rows in the reference-page table or independent path routes. Stable deep links use deterministic anchors on the owning page:

```text
baml.String.split
→ /baml/packages/v0.18.0/baml/String#split

baml.Color.Red
→ /baml/packages/v0.18.0/baml/Color#Red
```

The generator uses the member name as the anchor when it is unique on the page. When two exported members would collide, it appends a deterministic suffix derived from the stable exported ID. Search indexes nested members individually but links to their owning page and anchor.

Every routable page has a non-null qualified name. Its hierarchy is derived by splitting that name at the last `.`; the database does not store a parent or namespace column. For example, `baml.json.parse` has parent `baml.json`, while `baml` has no parent.

The packages published by the portal are explicitly selected in:

```text
content-data/reference/stdlib-packages.yaml
```

The initial membership is confirmed. The list is intentionally checked in because new standard-library packages are rare. The documentation generator runs the existing `baml describe <package> --export` command for every allowlisted package using the exact released toolchain. Introducing or retiring a public package must update the allowlist in the same change. The portal does not require a compiler API for package discovery.

Generated reference pages can include:

- Signature
- Documentation
- Parameters and return type
- Examples
- Availability and deprecation information
- Related definitions
- Link to the canonical source declaration

The page projection must preserve every docstring present in the package export, including docstrings for top-level declarations, fields, variants, required methods, default methods, explicit implementation methods, and implementation blocks. Associated types, generic parameters, function parameters, and associated-type bindings do not currently carry exported docstrings. Adding those optional fields later is an exporter enhancement, not an initial portal prerequisite.

The unversioned `/baml/packages` route is a generated package catalog, not a redirect. It provides a stable place to discover the latest stable release when one exists, browse every published package and version, inspect useful release and availability metadata, and follow links into exact-version reference pages or the changelog. During the initial bootstrap it lists the prepopulated canary and nightly snapshots without presenting either as stable.

### Language bridges

Language bridges explain how BAML concepts transition into an application's host language:

```text
/baml/bridges
/baml/bridges/[language]
```

For example:

```text
/baml/bridges/python
/baml/bridges/typescript
/baml/bridges/ruby
/baml/bridges/go
```

The exact subpage structure is intentionally deferred. Each bridge can begin as one canonical page and be divided only when the amount and shape of the content justify it.

A bridge page should focus on the contract between BAML and its host language:

- Supported BAML versions
- Bridge or runtime package versions
- Supported host-language versions
- Compatibility matrix
- BAML-to-host-language type mappings
- Input and output conversion behavior
- Nullability and optional-field semantics
- Union and enum representation
- Numeric behavior
- Date, time, media, and binary-data representation
- Streaming and asynchronous behavior
- Error and exception propagation
- Generated-name and reserved-keyword handling
- Known limitations
- Language-specific gotchas

Bridge pages remain unversioned. Compatibility is expressed through matrices and explicit version ranges rather than by copying the entire guide for every release.

Bridge snippets and host-language examples are not part of the initial automated testing scope.

## BAML CLI

The CLI documentation is tied to a particular BAML toolchain release:

```text
/cli/[version]
```

The initial structure is:

```text
/cli/v0.18.0
├── installation/
├── commands/
│   └── [command]/[subcommand]
├── configuration/
├── environment-variables/
└── exit-codes/
```

Command routes mirror actual CLI tokens:

```text
baml check
→ /cli/v0.18.0/commands/check

baml generate add
→ /cli/v0.18.0/commands/generate/add

baml auth login
→ /cli/v0.18.0/commands/auth/login

baml ide install
→ /cli/v0.18.0/commands/ide/install
```

Flags and options are anchors on command pages:

```text
/cli/v0.18.0/commands/run#project
```

The CLI generator should:

- Capture canonical help emitted by the exact released wrapper and toolchain binaries.
- Use canonical command names in URLs.
- Mirror subcommand nesting.
- Exclude hidden and internal commands from public navigation.
- Generate redirects for deprecated aliases when appropriate.
- Include arguments, flags, defaults, allowed values, examples, and exit behavior when those facts are present in canonical output.
- Allow authored guidance to appear around generated facts.

For the initial portal, this is a private documentation adapter over noncolored `baml help`, wrapper help, and version output. It retains the captured source text, parses only unambiguous fields, and fails when an expected help shape changes. A stable CLI metadata API and exhaustive typed configuration, environment-variable, and exit-code catalogs are deferred until a demonstrated documentation requirement justifies maintaining those contracts.

Installation and configuration live inside the versioned CLI section because supported installation methods, configuration schemas, environment variables, and defaults can change with the toolchain.

The unversioned `/cli` route is a useful CLI overview, not a redirect. It highlights the latest stable release when one exists while also providing installation guidance, a command catalog, all published versions, and release metadata before linking into exact-version documentation. During the initial bootstrap it lists the prepopulated canary and nightly snapshots without presenting either as stable.

CLI command examples are not executed as part of the documentation snippet-testing system. Initial CLI reference accuracy comes from capturing and parsing the canonical help emitted by the exact released binaries.

## Boundary Cloud Services

The formal product name is **Boundary Cloud Services**, abbreviated **BCS**.

Its reserved route is:

```text
/bcs
```

BCS is still being defined. The portal should begin with a coming-soon landing page rather than inventing documentation sections, APIs, or workflows before the product surface is known.

The initial page can contain:

- A concise description of the intended product area
- Current development or availability status
- A high-level statement of what BCS may help developers do
- Links to announcements, early access, or contact channels when available
- A clear indication that detailed documentation is forthcoming

Potential future sections—such as deployment, observability, debugging, APIs, or service limits—will be designed once the product and its developer workflows are concrete.

The portal navigation should display "Boundary Cloud Services." "BCS" can be used after the full name has been introduced.

## Cross-product content

Tutorials and examples remain top-level:

```text
/tutorials/[tutorial]
/examples/[example]
```

These sections are organized around developer goals rather than product boundaries.

They are not copied for every BAML release. Instead, pages can carry compatibility metadata such as:

```text
Tested with BAML 0.18.0
Requires BAML >= 0.18
Python 3.11+
```

Compatibility facts should come from one structured source rather than being repeated manually in frontmatter, prose, and UI components.

## Changelog

The portal has one changelog:

```text
/changelog
```

The canonical BAML language and toolchain changelog remains:

```text
baml_language/CHANGELOG.md
```

The portal parses and renders that file during the build:

```text
baml_language/CHANGELOG.md
        ↓
changelog loader
        ↓
/changelog
```

The developer portal must not maintain a separate copied `changelog.mdx`.

If the page needs durable introductory copy, that introduction may be authored under `content/`, but it must not duplicate release entries.

If additional historical release streams need to appear later, the changelog page should aggregate their canonical sources. It should not merge them into another manually maintained Markdown copy.

Individual versions can initially be headings with anchors:

```text
/changelog#v0-18-0
```

There is no separate `/releases` section. The changelog is authored release history and is not part of the package or CLI version selector.

## Repository location

The developer portal will live at:

```text
typescript2/app-developer-docs
```

It is a standalone Next.js application inside the existing `typescript2` pnpm/Turbo workspace.

The current workspace uses top-level `app-*` directories. It does not currently use an `apps/` directory. Creating `typescript2/apps/developer-docs` would introduce a second convention and require workspace configuration changes.

If the entire workspace is intentionally reorganized under `typescript2/apps/` later, the portal can move with that reorganization. The documentation project should not make that broader repository decision by itself.

The portal must not be added to `app-website`. It has a separate domain, information architecture, dependency profile, and release process.

The existing repository-root `docs/` and `fern/` directories are out of scope. The implementation should start from scratch and must never read, import, copy, or use either directory to infer the new information architecture or content.

## Application structure

The initial application structure is:

```text
typescript2/app-developer-docs/
├── app/                       # Next.js routes and layouts
├── content/                   # Canonical authored raw content
│   ├── baml/
│   │   ├── get-started/
│   │   ├── book/
│   │   ├── language/
│   │   └── bridges/
│   ├── tutorials/
│   ├── examples/
│   ├── bcs/
│   └── code/                  # Canonical BAML examples
├── content-data/              # Canonical structured editorial data
│   ├── bridges/
│   └── reference/
│       └── stdlib-packages.yaml
├── components/                # Portal-specific React components
├── lib/
│   ├── content/
│   ├── changelog/
│   ├── generated-content/
│   └── snippets/
├── scripts/                   # Validation and build entry points
├── public/
├── THIRD_PARTY_NOTICES.md     # Required notices for substantially copied code
├── next.config.ts
├── package.json
├── source.config.ts
└── tsconfig.json
```

Exact framework-required filenames may change, but the content ownership boundaries should remain.

The initial docs shell may be copied once from the local shadcn v4 reference described below. From the moment it is copied, the portal's local code is Boundary-maintained application code and is the only implementation source of truth. It is not a mirror of the reference repository; applicable third-party license notices remain intact.

## Source-of-truth policy

The portal must render canonical sources whenever possible. It must not introduce checked-in copies of facts already maintained elsewhere.

Every input falls into one of three categories.

### Canonical authored content

The portal is the source of truth, so the content is checked into Git under:

```text
content/
```

Examples include:

- Getting-started documentation
- BAML book chapters
- Language explanations
- Bridge explanations and gotchas
- Tutorials
- Example explanations
- BCS coming-soon content
- Canonical BAML code examples

### Canonical structured editorial data

Structured information authored specifically for documentation lives under:

```text
content-data/
```

Examples include:

- Bridge compatibility matrices
- Type-transition tables
- The explicit standard-package publication allowlist
- Explicit compatibility assertions
- Other structured facts that do not already have a canonical source elsewhere

A fact should appear in only one structured source. Pages and components render that source rather than restating it independently.

### Derived content

Derived data is published to PlanetScale Postgres as the generated-content store.

Examples include:

- Versioned standard-package exports
- Versioned CLI exports
- Generated navigation for those exports
- Source hashes and provenance metadata

PlanetScale is machine-authored. It is not a second location where humans manually edit the same facts. The connection string is supplied through a deployment secret such as `GENERATED_CONTENT_DATABASE_URL`; credentials must never appear in this proposal, checked-in configuration, logs, preview metadata, or client-side bundles.

The schema must support immutable exact-version records, mutable channel pointers, transactional publication, and build-time retrieval.

Every derived record must be traceable through its release and export relationships to the canonical product version, source commit, generator version, generation time, the applicable compiler- or portal-owned format version, and the appropriate authoritative content hash. The concrete columns are defined by the fixed schema below: `describe_sha256` for raw package exports and `source_sha256` plus `payload_sha256` for CLI artifacts. Reference-page projections inherit release and source provenance through `package_export_id` and identify their projection contract with `page_schema_version`.

The portal should fetch derived content from PlanetScale during the build and emit static pages. Core documentation should not require a live database query for every page view.

The portal has no filesystem-backed generated-content reader and does not persist a generated release bundle as a data source. Generation may use process memory and transient operating-system temporary directories, including the isolated directories required for standalone snippet checks. Any local caches or intermediate files are disposable, must not participate in portal rendering, and must be ignored by Git.

## PlanetScale schema

The generated-content schema is fixed before implementation so publishing and build-time consumers share one contract. It uses a dedicated PostgreSQL schema and five tables:

```text
developer_docs.channel_pointers
                  │
                  ▼
developer_docs.releases
       │                         │
       ▼                         ▼
package_exports             cli_artifacts
       │
       ▼
reference_pages
```

### Exact releases

`developer_docs.releases` contains one immutable row for each exact canonical BAML version. The database stores the canonical version without the URL's leading `v`.

```sql
CREATE SCHEMA IF NOT EXISTS developer_docs;

CREATE TABLE developer_docs.releases (
  version             TEXT PRIMARY KEY,
  source_commit       TEXT NOT NULL,
  released_at         TIMESTAMPTZ NOT NULL,
  generated_at        TIMESTAMPTZ NOT NULL,
  generator_version   TEXT NOT NULL,
  created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (version <> ''),
  CHECK (source_commit ~ '^[0-9a-f]{40}$')
);
```

There is no `status` or `channel` column. No row means the version is unpublished. A release row exists only when its complete package exports, current page projection, and CLI artifact are committed atomically.

### Channel pointers

`developer_docs.channel_pointers` is the only normally mutable generated-content table:

```sql
CREATE TABLE developer_docs.channel_pointers (
  channel          TEXT PRIMARY KEY,
  release_version  TEXT NOT NULL
    REFERENCES developer_docs.releases(version)
    ON DELETE RESTRICT,
  updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (channel IN ('stable', 'canary', 'nightly'))
);
```

Canary and nightly may reference the same release. Stable has no row until a stable artifact is published.

### Raw package exports

`developer_docs.package_exports` stores the exact authoritative `baml describe <package> --export` output for each allowlisted package and release:

```sql
CREATE TABLE developer_docs.package_exports (
  id                       BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  release_version          TEXT NOT NULL
    REFERENCES developer_docs.releases(version)
    ON DELETE RESTRICT,
  package_name             TEXT NOT NULL,
  describe_format_version  INTEGER NOT NULL,
  describe_output_json     TEXT NOT NULL,
  describe_sha256          TEXT NOT NULL,
  generated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

  UNIQUE (release_version, package_name),
  CHECK (package_name <> ''),
  CHECK (describe_format_version > 0),
  CHECK (describe_sha256 ~ '^[0-9a-f]{64}$')
);
```

The raw output is canonical JSON text rather than `JSONB`, preserving the exact bytes covered by `describe_sha256`.

### Projected reference pages

`developer_docs.reference_pages` contains the deterministic page projection derived from a package export:

```sql
CREATE TABLE developer_docs.reference_pages (
  package_export_id     BIGINT NOT NULL
    REFERENCES developer_docs.package_exports(id)
    ON DELETE RESTRICT,
  page_schema_version   INTEGER NOT NULL,
  qualified_name        TEXT NOT NULL,
  page_kind             TEXT NOT NULL,
  route_path            TEXT NOT NULL,
  page_data             JSONB NOT NULL,
  generated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

  PRIMARY KEY (
    package_export_id,
    page_schema_version,
    qualified_name
  ),

  UNIQUE (
    package_export_id,
    page_schema_version,
    route_path
  ),

  CHECK (page_schema_version > 0),
  CHECK (qualified_name <> ''),
  CHECK (route_path <> ''),
  CHECK (
    page_kind IN (
      'package',
      'namespace',
      'class',
      'enum',
      'interface',
      'type_alias',
      'function'
    )
  )
);
```

`route_path` is relative to `/baml/packages/v<version>/` and comes directly from the qualified-name segments:

```text
baml
baml/json
baml/json/parse
baml/String
```

The projection does not store a parent column because hierarchy is derived from the dotted qualified name. Nested members and implementations remain inside the owning page's `page_data` and use deterministic anchors.

The projection function declares a manually maintained `PAGE_SCHEMA_VERSION`. It is bumped whenever the same export would materially change page membership, data meaning, qualified names, route paths, or anchor generation. A new projection version appends rows; it never overwrites another projection version. The portal build explicitly selects the supported version.

### Raw CLI artifacts

CLI data is small enough to render directly and does not receive a reference-page projection table:

```sql
CREATE TABLE developer_docs.cli_artifacts (
  release_version          TEXT PRIMARY KEY
    REFERENCES developer_docs.releases(version)
    ON DELETE RESTRICT,
  wrapper_version          TEXT NOT NULL,
  artifact_schema_version  INTEGER NOT NULL,
  source_sha256            TEXT NOT NULL,
  payload_sha256           TEXT NOT NULL,
  payload_json             TEXT NOT NULL,
  generated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

  CHECK (wrapper_version <> ''),
  CHECK (artifact_schema_version > 0),
  CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
  CHECK (payload_sha256 ~ '^[0-9a-f]{64}$')
);
```

`source_sha256` covers the deterministic captured help inputs. `payload_sha256` covers the exact canonical `payload_json`, which contains the parsed command tree and raw help by command. The portal generates CLI routes directly from this artifact during its build.

### Publication and immutability

The same publisher implementation is callable from an operator-run script or CI/CD. It invokes one explicitly selected compiled `baml` CLI binary for every package and CLI operation in a publication run; only the orchestration and target database differ. It generates and validates all content in memory before opening the database transaction:

```text
Read and validate the exact version from the selected binary
        ↓
Generate all allowlisted package exports
        ↓
Generate package, namespace, and top-level declaration projections
        ↓
Generate the complete CLI artifact
        ↓
Validate schemas, routes, anchors, and hashes
        ↓
BEGIN
  Insert release
  Insert package exports
  Insert reference pages
  Insert CLI artifact
  Upsert optional channel pointer
COMMIT
```

If any operation fails, the transaction rolls back and no partial release is visible. A retry for an existing exact version succeeds only when the authoritative hashes and deterministic projection rows are identical; otherwise it is a hard failure. Published release, package-export, and CLI-artifact rows are never updated or deleted by normal tooling. Channel pointers may move only after their destination is complete.

Page projections are derived rather than authoritative. A new `PAGE_SCHEMA_VERSION` may be appended for historical package exports in a separate transaction before a portal deployment begins selecting it. Existing projection-version rows are not rewritten.

## Content provenance

| Portal content | Canonical source | Portal behavior |
|---|---|---|
| Getting started | Portal MDX | Render directly |
| BAML book | Portal MDX | Render and validate snippets |
| Language explanations | Portal MDX | Render directly |
| Language facts | Compiler metadata | Inject during build |
| Standard packages | Versioned compiler export in PlanetScale | Generate routes during build |
| CLI reference | Versioned CLI export in PlanetScale | Generate routes during build |
| Bridge guidance | Portal MDX | Render directly |
| Bridge compatibility | `content-data/bridges` | Render into bridge pages |
| Changelog | `baml_language/CHANGELOG.md` | Parse during build |
| BAML snippets | `content/code` | Compile and render the same source |
| Snippet IDs and expectations | Paths and embedded comments under `content/code` | Derive and validate in CI |
| Published standard-package selection | `content-data/reference/stdlib-packages.yaml` | Export each allowlisted package for an exact release |
| BAML grammar | Canonical grammar workspace package | Consume as a dependency |
| WASM runtime | Canonical workspace or release artifact | Consume later when runnable snippets and projects are implemented |
| BCS landing page | Portal MDX | Render directly |

## BAML code snippets

All displayed BAML examples have canonical source files under:

```text
content/code/
```

Pages reference examples by path-derived IDs rather than copying BAML code into MDX or repeating source paths in external manifests.

The directory structure distinguishes standalone files from proper projects:

```text
content/
└── code/
    ├── standalone/
    │   ├── hello-world.baml
    │   ├── optional-fields.baml
    │   └── invalid-return-type.baml
    └── projects/
        └── cross-file-example/
            ├── baml.toml
            └── baml_src/
                ├── types.baml
                └── main.baml
```

A page renders a standalone file or project with:

```mdx
<BamlSnippet id="hello-world" />
<BamlProject id="cross-file-example" />
```

The component type selects the source root. `BamlSnippet` resolves relative to `content/code/standalone/`; `BamlProject` resolves relative to `content/code/projects/`.

A standalone ID is the POSIX-style relative file path without `.baml`. A project ID is its POSIX-style relative directory path. For example:

```text
content/code/standalone/errors/invalid-return-type.baml
→ <BamlSnippet id="errors/invalid-return-type" />

content/code/projects/cross-file-example/
→ <BamlProject id="cross-file-example" />
```

Renaming or moving a source changes its ID and causes stale MDX references to fail CI. Duplicate derived IDs within either source root are validation errors. Standalone and project IDs occupy separate namespaces because they are resolved by different components.

The displayed region can be marked in the source:

```baml
// docs:start example
function ClassifyMessage(message: string) -> string {
  return message
}
// docs:end example
```

`BamlSnippet` displays the `example` region by default. It may select another named region explicitly. CI always compiles the complete source while the page displays only the selected region. `BamlProject` renders the proper project's file set without requiring another file list in structured data.

### Standalone files and proper projects

There are only two source shapes. Every `.baml` file beneath `content/code/standalone/` is a standalone example. Every directory addressed by a project ID beneath `content/code/projects/` is a proper BAML project containing:

```text
baml.toml
baml_src/
```

Multi-file examples must use that proper project structure. Arbitrary directories containing several `.baml` files are not supported as project fixtures.

The project validator must ensure:

- The project root contains `baml.toml`.
- The project root contains `baml_src/`.
- Every rendered file is inside the project.
- Project discovery and compilation use the same behavior as a normal BAML project.

### Expected compilation outcomes

Examples may be expected either to compile or to fail compilation. Success is the default and requires no metadata.

An intentionally invalid example embeds its expectation in the same `.baml` file:

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

The metadata block is YAML carried in ordinary `//` comments. The parser strips exactly one comment prefix from each line and parses the remaining text. Plain comments prevent the metadata from becoming documentation attached to a BAML declaration. The metadata remains part of the complete file passed to the compiler but sits outside the displayed region.

A standalone file may contain at most one `docs:meta` block. A project may contain at most one such block across all `.baml` files under `baml_src/`; that expectation applies to the complete project. Multiple blocks, unknown keys, malformed YAML, or contradictory expectations fail CI.

CI verifies both directions:

- A success example fails CI if it no longer compiles.
- A failure example fails CI if it unexpectedly compiles.
- A failure example fails CI if it fails for the wrong reason.

Negative examples should assert at least a diagnostic code when one exists. Merely expecting any failure could pass because of an unrelated syntax error.

Exact diagnostic snapshots are optional because complete wording and source spans can be brittle. Prefer diagnostic codes plus targeted message fragments.

Each standalone file or project has one expected compilation outcome. Examples that need different outcomes use separate source files or projects.

### Validation implementation

Snippet validation is portal-owned tooling and requires no new compiler interface. A strict parser recognizes only the comment-delimited YAML metadata and the `docs:start` and `docs:end` region markers. It does not parse BAML syntax.

For a standalone source, the validator creates a fresh directory using the operating system's temporary-directory API, copies only that complete `.baml` file into it, and runs the existing command:

```text
baml \
  --output-preset agent \
  --color never \
  --no-progress \
  --diagnostic-format agent \
  check \
  --project <temporary-directory>
```

The validator must not hard-code `/tmp`; for example, a TypeScript implementation uses `os.tmpdir()` with `fs.mkdtemp()` and cleans up in a `finally` block. Isolating each standalone file prevents sibling `.baml` files from participating in the check.

A proper multi-file fixture is checked directly with `baml check --project <project-directory>`. Expected success or failure comes from the process exit status. Expected failures additionally match existing diagnostic codes and optional message fragments in agent output. If the small output parser cannot understand the compiler output, CI fails and prints the captured output.

### Snippet-testing scope

Automated snippet validation is intentionally limited to BAML source examples.

It does not initially test:

- CLI command examples
- Shell commands
- Bridge generation
- Host-language snippets
- Bridge compatibility matrices
- Host-language type checking

CLI reference accuracy comes from captured exact-version CLI output. Bridge guidance and examples remain authored documentation.

Executable BAML examples should normally use `BamlSnippet` or `BamlProject`. Intentionally noncompiled BAML-looking content must be explicitly marked as illustrative.

## Versioning policy

Only these sections track the BAML toolchain version:

```text
/baml/packages/[version]
/cli/[version]
```

They are versioned because their generated reference must exactly match the toolchain a developer has installed.

Canonical URLs contain the exact release:

```text
/baml/packages/v0.18.0/...
/cli/v0.18.0/...
```

The rules are:

- Every published exact-version generated record is immutable after publication, including stable, canary, and nightly releases.
- Historical data comes from the release artifact belonging to that version.
- An old version's authoritative package exports and CLI artifact must never be regenerated using current compiler or CLI source.
- A new `PAGE_SCHEMA_VERSION` may be projected for an old version only from its stored immutable package exports; the new projection rows are appended and the prior projection rows remain unchanged.
- The version selector appears only in package and CLI sections.
- Switching versions preserves the current definition or command when it exists.
- If the destination does not exist, the portal explains that explicitly.
- The portal must never silently substitute the latest version.
- Search defaults to the latest stable version when one exists, with older versions available as a filter.
- Until a stable record is populated, search and the unversioned catalogs expose canary and nightly as explicit noindex channel snapshots without treating either as stable.
- Initial database population resolves the current canary and nightly channels to their exact canonical versions, publishes immutable package and CLI records for both, and then creates the two mutable channel pointers.
- Channel pointer changes never mutate the exact-version records they reference.

Generation and versioning are separate decisions. Content is versioned when developers need historical documentation matching an installed toolchain—not simply because it was generated.

## Platform

The portal is one Next.js application using Fumadocs:

```text
Next.js
├── Fumadocs layouts and navigation
├── MDX content
├── Shiki syntax highlighting
├── BAML React components
├── generated-content client
├── snippet validation scripts
└── unified search
```

Fumadocs provides documentation primitives while leaving the application, rendering pipeline, React components, and build process under Boundary's control.

## Design and layout baseline

The first implementation should use the shadcn v4 documentation application as a one-time visual and structural baseline:

```text
/Users/vbv/repos/reference/shadcn-ui/apps/v4
```

The reference was inspected at commit `63c1308d112b6b1205d86244a156cca1abef5087`. It already combines Next.js, Fumadocs, MDX, Tailwind CSS, static route generation, and contextual not-found behavior in a way that closely matches this portal's needs. Starting from that proven docs experience reduces early design invention while retaining the application ownership and customization required for BAML.

The initial implementation should copy and adapt the generally useful documentation shell:

- Sticky site header and primary navigation
- Search command menu
- Responsive mobile navigation
- Left documentation sidebar with active and scroll state
- Narrow primary reading column
- Right-hand table of contents on suitable pages
- Breadcrumbs
- MDX typography and code-block presentation
- Copy-page and raw-Markdown actions where useful
- Previous and next page navigation
- Light and dark themes
- Useful section index pages
- Responsive behavior
- Static generation and contextual 404 patterns

The default desktop reading layout should follow the same restrained three-column model:

```text
┌──────────────────────────────────────────────────────────────┐
│ sticky header: identity · navigation · search · actions     │
├────────────────┬───────────────────────────┬─────────────────┤
│ section        │ breadcrumbs               │ on this page    │
│ navigation     │ title and introduction    │                 │
│                │ article content           │                 │
│                │ code and examples         │                 │
│                │ previous / next           │                 │
└────────────────┴───────────────────────────┴─────────────────┘
```

This is a BAML-branded adaptation, not a shadcn-branded site. Preserve the reference's spacing, density, typography hierarchy, navigation ergonomics, responsive behavior, and theme quality while replacing its identity and information architecture. Use the BAML logo and product names throughout. A restrained monochrome base with Boundary purple used sparingly for identity and meaningful state is preferred over a marketing-heavy treatment. Documentation readability takes precedence over decorative animation.

Do not copy product-specific features that do not serve this portal, including:

- Registry infrastructure and registry health surfaces
- Component previews and component-installation workflows
- Base-color or component-style switchers
- The shadcn designer
- "Open in v0" actions
- Blocks, charts, or other shadcn catalog experiences
- Shadcn-specific AI shortcuts

The copy is a one-time bootstrap. Once copied, Boundary maintains the local implementation independently and may modify, delete, or rewrite any part of it. There is no ongoing upstream relationship, synchronization process, upgrade path, patch queue, re-upload workflow, or requirement to preserve file-level similarity. Do not add sync tooling, an `UPSTREAM.md`, or per-file upstream-tracking comments.

Dependency versions must be selected for compatibility with the `typescript2` workspace rather than copied blindly. Because the shadcn reference is MIT-licensed, retain the legally required copyright and license notice for any substantially copied portions in `THIRD_PARTY_NOTICES.md`. That notice records provenance and licensing; it does not make the reference repository a continuing source of truth.

### Why not a managed platform?

Mintlify and Fern were considered but are not considered good fits or planned fallbacks.

Boundary's custom requirements are central:

- Language-specific highlighting
- Compiler-generated package reference
- Generated CLI reference
- BAML snippet compilation
- Runnable WASM examples
- Interactive book components
- Version synchronization
- Potential future compiler or language-server integration

A managed platform would make these core behaviors depend on vendor extension points and platform constraints.

If Fumadocs proves unsuitable, the fallback is still an internally owned Next.js application with a different content layer—not a default migration to Mintlify or Fern.

## BAML artifacts

The portal should consume canonical language artifacts rather than copying them.

For the initial launch, this includes:

- The BAML grammar from the existing grammar workspace package
- Versioned package metadata from PlanetScale
- Versioned CLI metadata from PlanetScale

BAML WASM can be added shortly after launch as an enhancement to `BamlSnippet` and `BamlProject`. When added, it must come from its canonical workspace or release artifact rather than a copied runtime.

The release flow is:

```text
BAML release
    ↓
Publish the exact compiled CLI binary and canonical grammar
    ↓
Run the shared documentation publisher with that binary
    ↓
Generate and validate package and CLI metadata
    ↓
Store the complete immutable exact release transactionally
    ↓
Update the applicable channel pointer
    ↓
Compile documentation snippets with the selected binary
    ↓
Build documentation preview
    ↓
Publish production documentation
```

This keeps the grammar, package reference, CLI reference, and documented release aligned. The same version alignment applies to the WASM runtime once runnable examples are introduced.

## Implementation boundaries

An implementation agent must follow these rules:

1. Create a new application at `typescript2/app-developer-docs`.
2. Do not add portal routes to `app-website`.
3. Do not create a new `typescript2/apps/` hierarchy as part of this work.
4. Never read, import, copy, or use the existing repository-root `docs/` or `fern/` directories as source material.
5. Keep canonical authored raw content under `content/`.
6. Keep canonical structured editorial data under `content-data/`.
7. Publish derived versioned data to PlanetScale; do not hand-edit it.
8. Do not check local generated caches or build artifacts into Git.
9. Keep the allowlist-driven package generator, CLI capture/parser, and snippet validator inside `app-developer-docs`; do not add compiler-side documentation tooling.
10. Consume shared TypeScript packages through declared workspace dependencies rather than deep sibling imports.
11. Keep portal-specific components inside `app-developer-docs`.
12. Do not create speculative BCS routes.
13. Do not create speculative bridge subpages.
14. Do not create an integrations section.
15. Do not introduce a copied changelog.
16. Do not build CLI or bridge example-testing infrastructure in the initial implementation.
17. Use standalone BAML files by default; use only proper BAML projects for multi-file examples.
18. Fail CI when displayed BAML snippets do not produce their declared compilation outcome.
19. Limit cross-workspace edits to necessary workspace, lockfile, build, CI, and shared-artifact integration changes.
20. Keep the initial application static-first; do not require live database queries to render ordinary pages.
21. Implement the explicit route behavior in this proposal; do not invent floating-version redirects or silent fallbacks.
22. Use the local shadcn v4 docs application only as a one-time implementation and design baseline.
23. After the initial copy, treat the portal's local implementation as canonical; do not add upstream sync, update, or tracking machinery.
24. Copy only generally useful docs-shell behavior and omit shadcn-specific product features.
25. Preserve required MIT attribution for substantially copied portions in `THIRD_PARTY_NOTICES.md`.
26. Generate package and CLI reference by invoking one explicitly selected compiled `baml` CLI binary; do not import compiler internals as an alternate generation path.
27. Use Postgres as the generated-content source in every environment; do not add a filesystem generated-content reader or persistent release bundle.
28. Keep manual and CI/CD publication on the same generator, validator, and transactional publisher implementation.

## CI and deployment

Use GitHub Actions and Vercel:

```text
Canonical sources
      ↓
Validate authored content and BAML snippets
      ↓
Fetch generated package and CLI data
      ↓
Verify provenance and versions
      ↓
Next.js/Fumadocs build
      ↓
Vercel preview or production deployment
      ↓
developer.boundaryml.com
```

Pull requests should:

1. Validate MDX and structured content-data schemas.
2. Resolve every `BamlSnippet` and `BamlProject` reference from its path-derived ID.
3. Verify every referenced source file and region exists.
4. Detect duplicate derived snippet or project IDs.
5. Compile every expected-success BAML example.
6. Verify every expected-failure example fails with the expected diagnostic.
7. Validate generated route uniqueness.
8. Fetch and validate required generated-content records.
9. Check internal links and redirects.
10. Build the complete portal.
11. Deploy a noindex preview.

Snippet validation should run when any of these change:

- Portal content or content-data
- BAML compiler
- Standard packages
- BAML grammar
- Snippet-validation infrastructure

Compilation can be cached using:

```text
source hash
+ compiler version
+ validator version
```

Failures should identify:

- Documentation page
- Snippet ID
- Source path
- Toolchain version
- Expected outcome
- Actual diagnostic

Merges to the production branch should:

1. Run the required checks.
2. Build one production artifact.
3. Deploy atomically.
4. Smoke-test navigation, search, syntax highlighting, and generated reference.

`docs.boundaryml.com` should permanently redirect to corresponding pages on `developer.boundaryml.com`.

## Platform ownership and risks

Owning the application gives Boundary control, but also creates responsibility for:

- Framework and dependency upgrades
- Performance
- Accessibility
- Search configuration
- Redirects
- Analytics
- Browser testing
- PlanetScale reliability
- Custom component reliability

The highest post-launch enhancement risk is the BAML runner. It is not an initial-launch prerequisite. When implemented, the WASM artifact will need:

- Lazy loading
- Appropriate browser caching
- Worker isolation
- Version synchronization
- Clear failure states
- Tests preventing the runtime and displayed documentation from drifting

The generated-content pipeline also needs stable schemas. The portal should consume a versioned metadata contract rather than depending directly on compiler internals.

## Initial architecture proof

Before migrating substantial content, prove the architecture with:

1. The `typescript2/app-developer-docs` application scaffold.
2. The one-time adapted shadcn docs shell with desktop, mobile, light, and dark layouts.
3. One complete book chapter.
4. The canonical BAML grammar through Shiki.
5. One valid standalone BAML snippet compiled in CI.
6. One intentionally invalid BAML snippet with an expected diagnostic.
7. One proper multi-file BAML project containing `baml.toml` and `baml_src/`.
8. The `/baml/packages` catalog plus one generated, versioned standard-package namespace loaded from PlanetScale.
9. The `/cli` overview plus one generated, versioned CLI command tree loaded from PlanetScale.
10. One bridge page with a compatibility matrix and type-transition table.
11. The BCS coming-soon landing page.
12. `/changelog` rendered directly from `baml_language/CHANGELOG.md`.
13. Search across authored and generated content.
14. Contextual 404s for an unknown version, an unversioned `/baml/vMAJOR.MINOR.PATCH` path, and a speculative deeper BCS or bridge path.
15. One Vercel pull-request preview.

## Decisions

| Question | Decision | Reason |
|---|---|---|
| `docs` or `developer`? | `developer.boundaryml.com` | It represents the complete technical ecosystem. |
| One portal or several sites? | One portal | Separate sites would fragment navigation, search, previews, analytics, and deployment. |
| Where does the code live? | `typescript2/app-developer-docs` | It follows the existing workspace convention and can reuse grammar, WASM, testing, and deployment infrastructure. |
| Put it in `app-website`? | No | The developer portal has its own domain, architecture, and release process. |
| Use legacy documentation as input? | No | The new portal and information architecture start from scratch. |
| Product-first or content-first? | Hybrid | BAML, CLI, and BCS are product surfaces; tutorials and examples cross boundaries. |
| Top-level `/reference`? | No | Reference belongs to the product that defines it. |
| Separate book site? | No | The book is part of BAML and should share portal infrastructure. |
| `/stdlib` or `/packages`? | `/baml/packages` | The standard library is exposed as BAML packages. |
| What does `/baml/packages` do? | Render a generated catalog | It is the durable discovery page for packages, versions, releases, availability, and related metadata—not a redirect. |
| Add definition-kind segments to package URLs? | No | Routable qualified names map directly to path segments. `page_kind` is rendering metadata only. |
| Do members receive independent pages? | No | Fields, methods, variants, associated types, and impl details remain on the owning top-level declaration page with deterministic anchors. |
| Store page parents? | No | The immediate parent is derived by removing the final segment of the required dotted qualified name. |
| What does `/cli` do? | Render a useful overview | It introduces installation, commands, versions, and release metadata—not a redirect. |
| How do section roots behave? | Render useful pages | Recognized roots are summaries, catalogs, or tables of contents rather than automatic forwards to a child page. |
| What happens for unknown or inapplicable routes? | Return a contextual 404 | Helpful recovery links are allowed, but the portal never silently substitutes a different page or version. |
| Cloud naming? | Boundary Cloud Services, abbreviated BCS | The full name is clearer than the generic "Cloud"; its route is `/bcs`. |
| Define BCS documentation now? | No | Start with a coming-soon page until the product surface is known. |
| Define bridge subpages now? | No | Begin with one page per language and let the content determine future subdivisions. |
| Integrations section? | No | There are currently no integrations to document. |
| `/releases` or `/changelog`? | `/changelog` | Render the canonical changelog without creating another release hierarchy. |
| Which documentation is versioned? | Standard packages and CLI | These references must match the installed toolchain. |
| Where does authored raw content live? | `content/` | It is the canonical source for human-authored documentation. |
| Where does authored structured data live? | `content-data/` | It provides one canonical structured source for matrices, mappings, compatibility assertions, and the package publication allowlist. |
| Where does derived data live? | PlanetScale Postgres | Generated versioned data is machine-published outside Git and fetched at portal build time. |
| How is package and CLI reference generated? | One explicitly selected compiled `baml` CLI binary | The exact executable is the canonical source for every package export, CLI help capture, and version in one publication run. |
| Can the portal read generated content from the filesystem? | No | Postgres is the generated-content source in every environment; in-memory generation and transient snippet directories are not portal data sources. |
| Are manual and CI/CD publishers separate implementations? | No | Both use the same generator, validator, and atomic publisher; only orchestration and the target database differ. |
| Track building or failed releases in PlanetScale? | No | Content is prepared before one atomic transaction; a release row exists only for a complete published version. |
| How are package pages stored? | Versioned projection rows | Raw package exports remain authoritative; page projections are appended under a manually bumped `PAGE_SCHEMA_VERSION`. |
| Project CLI pages into rows? | No | One exact-version CLI JSON artifact is small enough for the portal build to render directly. |
| Where do BAML snippets live? | `content/code/` | The same canonical source is compiled and rendered. |
| How are snippets validated? | Standalone file or proper project | Standalone files are the default; multi-file examples use `baml.toml` and `baml_src/`. |
| Where do snippet IDs and expectations live? | Source paths and embedded `.baml` comments | This avoids repeating IDs, source paths, or expected outcomes in external manifests. |
| Can examples intentionally fail? | Yes | Embedded metadata declares expected diagnostics, and CI verifies the outcome. |
| How are published packages selected? | Checked-in allowlist | Package additions are rare, so explicit publication configuration is simpler than compiler discovery infrastructure. |
| Test CLI and bridge examples? | No | Initial automated snippet validation is limited to BAML source. |
| Is the WASM runner required for initial launch? | No | It follows shortly after launch as an enhancement to `BamlSnippet` and `BamlProject`. |
| Which versions are populated initially? | Current canary and nightly exact versions | Their immutable records are prepopulated in PlanetScale and referenced by mutable channel pointers; neither is presented as stable. |
| Managed platform or internal application? | Internal Fumadocs application | The language, generation, WASM, and versioning requirements require control. |
| Initial design baseline? | One-time copy and adaptation of shadcn v4 docs | Its Next.js and Fumadocs shell supplies a proven layout and interaction model without constraining later ownership or customization. |
| Maintain an upstream shadcn relationship? | No | The local copy becomes canonical immediately; there is no sync, upgrade, patch, or re-upload process. |
| How is copied code attributed? | `THIRD_PARTY_NOTICES.md` | Preserve the required MIT notice without turning the reference repository into a second source of truth. |
| Mintlify or Fern as fallback? | No | They were evaluated, but the architectural mismatch remains. |

## Outcome

The result is one developer destination with clear product boundaries, a first-class BAML book, continuously compiled BAML examples, precise versioned package and CLI reference, minimal duplication of canonical information, and room for BCS and bridge documentation to develop as their real requirements become known.
