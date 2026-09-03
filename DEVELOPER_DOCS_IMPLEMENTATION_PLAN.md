# Developer Documentation Implementation Plan

## Purpose and authority

This document turns [DEVELOPER_DOCS_PROPOSAL.md](./DEVELOPER_DOCS_PROPOSAL.md) and [DEVELOPER_DOCS_COMPILER_ENABLEMENT.md](./DEVELOPER_DOCS_COMPILER_ENABLEMENT.md) into an ordered implementation plan.

The proposal remains authoritative for product scope, routes, content ownership, design direction, and storage semantics. The compiler-enablement supplement remains authoritative for the boundary between the portal and BAML tooling. This plan defines execution order, parallel work, checkpoints, and human approval gates. It must not silently change either specification.

Implementation progress, verification evidence, gate state, and evidence-backed deviations are recorded in [DEVELOPER_DOCS_STATUS.md](./DEVELOPER_DOCS_STATUS.md). The status document reports execution; it does not override this plan, the proposal, or the compiler-enablement supplement.

## Outcome

The initial implementation produces:

- A new Next.js and Fumadocs application at `typescript2/app-developer-docs`.
- A BAML-branded, one-time adaptation of the shadcn v4 documentation shell.
- Authored documentation from `content/` and structured editorial data from `content-data/`.
- Continuously checked `BamlSnippet` and `BamlProject` examples.
- Versioned standard-package and CLI reference generated from one exact compiled `baml` CLI binary.
- Operator-run database tooling that can populate and verify a local, development, preview, or production Postgres database without invoking the CI/CD release workflow.
- PlanetScale Postgres as the only generated-content source used by the portal.
- Static production output, Vercel previews, search, contextual 404s, and the agreed launch routes.

## Non-negotiable constraints

1. Never read, import, copy, or infer behavior from the existing repository-root `docs/` or `fern/` directories.
2. Do not modify the compiler or add a public documentation command, file-checking command, diagnostic protocol, or documentation SDK for the initial portal.
3. Generate all package and CLI data by invoking one explicitly selected compiled `baml` CLI binary.
4. Do not import compiler internals as an alternate generation path.
5. Do not implement a filesystem-backed generated-content mode for the portal.
6. Do not persist generated release bundles as a portal data source. Generation may use process memory and transient operating-system temporary directories only.
7. The portal reads generated content through the Postgres contract in every environment. Production pages remain static-first and fetch generated content during the build rather than on ordinary browser requests.
8. Keep database credentials exclusively in environment variables. Never accept or print credentials in command arguments, source files, logs, artifacts, or client bundles.
9. Keep exact-version releases and authoritative artifacts immutable. Update channel pointers only after a complete release is present.
10. Do not add `kind` to standard-package URLs, independent member routes, stored page parents, release publication statuses, or a CLI page-projection table.
11. Do not create integrations, speculative BCS routes, or speculative bridge subpages.
12. Do not add the WASM runner to the launch critical path.

Transient temporary directories used to isolate standalone BAML snippet checks are allowed. They are compiler inputs, not a generated-content store or portal data source.

## Execution overview

```text
Phase 0: preflight and application scaffold
                    │
                    ▼
Phase 1: concrete database and payload contracts
                    │
             HUMAN GATE 1
                    │
        ┌───────────┼───────────┬───────────┬───────────┐
        ▼           ▼           ▼           ▼           ▼
   Docs shell    Snippets    Packages      CLI       Authored
   and routes     and CI     generator   generator   content
        └───────────┴───────────┴───────────┴───────────┘
                    │
                    ▼
Phase 3: publisher, database consumers, search, and integration
                    │
                    ▼
Phase 4: complete architecture-proof preview
                    │
             HUMAN GATE 2
                    │
                    ▼
Phase 5: content expansion and production hardening
                    │
                    ▼
Phase 6: canary/nightly population and launch candidate
                    │
             HUMAN GATE 3
                    │
                    ▼
Phase 7: production deployment and redirects
```

## Phase 0: preflight and scaffold

This phase is serialized because every later workstream depends on the same workspace, application, and dependency choices.

### Work

1. Confirm the new application path is `typescript2/app-developer-docs`.
2. Inspect only the relevant `typescript2` workspace conventions and the local shadcn v4 reference application.
3. Select Next.js, Fumadocs, MDX, Shiki, database, validation, and test dependencies compatible with the existing workspace.
4. Create the application and declare workspace dependencies without deep sibling imports.
5. Establish the initial directories:

   ```text
   typescript2/app-developer-docs/
   ├── app/
   ├── components/
   ├── content/
   ├── content-data/
   ├── lib/
   ├── scripts/
   └── tests/
   ```

6. Add minimal lint, type-check, test, and production-build commands.
7. Add a minimal GitHub Actions check and Vercel preview path.
8. Copy only the generally useful shadcn documentation-shell behavior and add the required MIT notice to `THIRD_PARTY_NOTICES.md`.
9. Add guardrails preventing local generated caches, secrets, and temporary files from entering Git.

### Checkpoint

- The empty portal builds inside the existing workspace.
- A pull request can produce a noindex preview.
- The application contains no imported legacy documentation and no shadcn upstream-sync machinery.

## Phase 1: concrete contracts and database tooling skeleton

This phase is serialized because all parallel producers and consumers must agree on the same concrete representation.

### Database contract

1. Create the migration for the five tables defined in the proposal:

   ```text
   developer_docs.releases
   developer_docs.channel_pointers
   developer_docs.package_exports
   developer_docs.reference_pages
   developer_docs.cli_artifacts
   ```

2. Add TypeScript validators for database rows and the versioned JSON payloads.
3. Set the initial manually maintained `PAGE_SCHEMA_VERSION`.
4. Implement canonical JSON serialization and SHA-256 hashing utilities.
5. Implement the FQN-to-route and deterministic member-anchor functions.
6. Implement the build-time Postgres reader used by the portal.
7. Add migration, inspection, and verification command skeletons.

### PlanetScale CLI availability

The implementation environment already has the `pscale` CLI installed and authenticated. PlanetScale control-plane and schema operations may use that authenticated CLI in the `boundaryml` organization.

The following rules apply:

- Use read-only `pscale` discovery to enumerate the available `boundaryml` databases and branches before selecting a target.
- Never infer the write target from whichever database or branch happens to be active. Migration and population commands require an explicit organization, database, and branch or equivalent unambiguous target.
- Keep the reviewed SQL migration in the repository as the canonical schema definition. The live PlanetScale schema is an applied result, not a separately authored source of truth.
- Use the `pscale` CLI and its supported Postgres connection workflow to create the selected database or branch when required, apply the reviewed schema, and verify the resulting tables and constraints.
- Do not place authenticated CLI state, access tokens, generated connection strings, or passwords in the repository, command output captured by CI, or documentation artifacts.
- CLI authentication is for operator and control-plane operations. The portal and population code still receive their Postgres connection through an environment variable such as `GENERATED_CONTENT_DATABASE_URL`.
- Do not use ad hoc `pscale` or SQL commands to hand-edit generated release rows. Exact-version content and channel pointers continue to flow through the shared transactional publisher.
- Database or branch creation and the first schema application are explicit operations performed only after the Human Gate 1 migration review identifies the precise target. Subsequent non-destructive applications of that approved migration may be automated.

### Concrete sample contracts

Use a real compiled `baml` binary to produce representative data for review. At minimum, exercise:

- One package page.
- One namespace page.
- One class or interface containing nested members, implementations, and docstrings.
- One top-level function.
- One CLI root with nested commands, arguments, and flags.

Define and validate the initial concrete shapes of `reference_pages.page_data` and `cli_artifacts.payload_json` from those real outputs. The samples are review inputs, not a checked-in generated-content source.

### Operator-run database commands

The initial command surface should provide capabilities equivalent to:

```text
pnpm docs:db:migrate
pnpm docs:db:populate --baml-bin <absolute-path> --source-commit <full-sha> --released-at <iso-timestamp> [--channel <channel>]
pnpm docs:db:populate --baml-bin <absolute-path> --source-commit <full-sha> --released-at <iso-timestamp> [--channel <channel>] --dry-run
pnpm docs:db:inspect --version <canonical-version>
pnpm docs:db:verify --version <canonical-version>
```

Exact names may follow workspace conventions, but the following behavior is required:

- `--baml-bin` identifies one compiled CLI binary used for every describe, help, and version invocation in that run, plus any snippet checks performed by the same workflow.
- The script obtains the canonical product version from that binary and rejects contradictory caller-supplied version data.
- `--source-commit` is the full source commit and is explicit unless the selected binary exposes trustworthy equivalent metadata.
- `--released-at` is the actual release time and is explicit unless the selected binary exposes trustworthy equivalent metadata. It must not default to the publication time.
- `wrapper_version` comes from the selected binary's version output; publication fails if the required binary identity cannot be established.
- `generator_version` comes from the documentation tooling revision, while `generated_at` records the population execution time.
- The database URL comes only from an environment variable such as `GENERATED_CONTENT_DATABASE_URL`.
- `--dry-run` performs generation and complete validation but makes no database writes and leaves no persistent bundle.
- Population generates all package and CLI data before opening the publication transaction.
- Population inserts the release, package exports, page projections, and CLI artifact before moving the optional channel pointer.
- A retry of an existing exact version succeeds only when authoritative hashes and rows are identical.
- Normal population never updates or deletes an exact-version release.
- Any destructive local-database reset utility is separate, explicitly local-only, and unable to target a shared or production database.
- Production publication requires an explicit production context in addition to the presence of a database URL.

### Human gate 1: stored and rendered contract approval

This gate has two ordered checkpoints:

1. **Target and migration approval.** A human reviews the exact `boundaryml` organization, database, development branch, and SQL migration. After approval, apply the migration to that isolated development target with the authenticated `pscale` workflow.
2. **Payload and rendering approval.** Publish the representative exact-version sample through the shared population command, then review the stored payloads and Postgres-backed rendered pages.

Stop before expanding the generators or renderers. Across the two checkpoints, a human reviews:

1. The actual SQL migration.
2. The concrete package and CLI JSON payloads.
3. One rendered package, namespace, and declaration page.
4. Nested-member anchors, docstrings, cross-references, and search targets.
5. One rendered CLI command tree.
6. Dry-run and transaction summaries with no credentials or raw secret values.

Approval means the concrete encoding can support the intended product. It does not reopen previously settled architecture without implementation evidence of a conflict.

## Phase 2: parallel workstreams

After Human Gate 1, the following workstreams can proceed concurrently. Each workstream should stay within its ownership boundary to reduce merge conflicts.

### Workstream A: documentation shell and static routes

Owns:

- Shared layout, header, sidebars, table of contents, breadcrumbs, and previous/next navigation.
- Responsive behavior and light/dark themes.
- BAML branding and visual tokens.
- Landing pages, catalogs, contextual 404s, and route-level metadata.
- Static route behavior for BAML, CLI, BCS, tutorials, examples, and changelog.

Does not own generated data loaders or generators.

### Workstream B: BAML snippets

Owns:

- Embedded YAML metadata parsing.
- Region-marker parsing.
- Path-derived `BamlSnippet` and `BamlProject` IDs.
- Per-file operating-system temporary-directory isolation for standalone files.
- Proper-project validation for `baml.toml` plus `baml_src/`.
- Expected-success and expected-failure checks using the selected compiled CLI binary.
- CI diagnostics that identify the page, snippet, source, version, expectation, and actual result.

Does not test CLI or bridge examples and does not add compiler APIs.

### Workstream C: package generation

Owns:

- The confirmed `content-data/reference/stdlib-packages.yaml` allowlist.
- Exact-binary `baml describe <package> --export` invocation.
- Raw output preservation and hashing.
- Package, namespace, and top-level declaration page projection.
- Nested-member, implementation, and docstring preservation.
- FQN route, anchor, and cross-reference validation.
- In-memory package publication input returned to the shared publisher.

Does not publish independently or create `kind` or member path segments.

### Workstream D: CLI generation

Owns:

- Public wrapper and toolchain help capture through entry points exposed by the same selected binary.
- Recursive traversal of known public entry points.
- Minimal parsing of unambiguous emitted facts.
- Retention and hashing of raw help inputs.
- One canonical versioned CLI publication input returned in memory to the shared publisher.

Does not add a compiler or CLI metadata API, invent undocumented facts, or create a CLI projection table.

### Workstream E: authored content and canonical loaders

Owns:

- MDX loading from `content/`.
- Structured editorial loading from `content-data/`.
- One complete book chapter.
- One bridge page with a compatibility matrix, transition rules, and gotchas but no speculative subpages.
- The BCS coming-soon landing page and no deeper BCS routes.
- Tutorials and examples indexes.
- `/changelog` rendered directly from `baml_language/CHANGELOG.md` without a copied changelog.

### Workstream F: CI and preview infrastructure

Owns:

- Independent jobs for authored-content validation, snippets, route validation, generated-content verification, link checking, and the complete build.
- Caches keyed by the appropriate source, compiler, and validator versions.
- Noindex Vercel previews.
- Secret handling and log redaction.

This workstream coordinates package scripts and shared CI configuration with the other workstreams rather than creating alternate validation implementations.

## Phase 3: serialized integration

The parallel outputs converge in this order:

1. Combine the package and CLI generators behind one operator-run population command.
2. Generate all data in process memory and validate completeness, schemas, routes, anchors, hashes, and provenance.
3. Implement the single atomic publication transaction.
4. Implement immutable-retry verification and channel-pointer movement.
5. Populate an isolated development database with one exact compiled CLI version.
6. Connect package catalogs, package routes, CLI catalogs, and CLI routes to the build-time Postgres reader.
7. Normalize authored and generated pages into one search index.
8. Add internal-link, cross-reference, missing-record, and contextual-404 tests.
9. Build and deploy the complete integrated preview.

Package and CLI generation may execute concurrently before publication. Database publication is one serialized transaction. Search integration follows route finalization because search results must contain canonical URLs and member anchors.

### Projection rollout rule

For a future page-projection change:

```text
Implement and validate new PAGE_SCHEMA_VERSION
                    ↓
Append projections for every required stored package export
                    ↓
Verify historical and current release coverage
                    ↓
Deploy the portal consumer that selects the new version
```

Never deploy a consumer before the selected projection version is available for every version it must serve.

## Phase 4: architecture proof

The integrated preview must contain:

1. The adapted documentation shell on desktop and mobile in light and dark themes.
2. One complete book chapter using the canonical BAML grammar.
3. One valid standalone snippet.
4. One intentionally invalid standalone snippet with its expected diagnostic.
5. One proper multi-file project with `baml.toml` and `baml_src/`.
6. `/baml/packages`, an exact-version package index, a namespace, and representative top-level declaration pages.
7. Nested members with deterministic anchors and searchable owner-page links.
8. `/cli`, an exact-version CLI overview, and one complete command subtree.
9. One bridge compatibility page.
10. The BCS coming-soon page.
11. `/changelog` from its canonical source.
12. Search across authored and generated content.
13. Contextual 404s for unknown versions and unsupported BAML, bridge, and BCS paths.
14. A noindex Vercel preview.

### Human gate 2: experience and architecture approval

Stop before substantial content expansion. A human reviews:

- Navigation and information architecture.
- Reading experience, density, typography, themes, and responsive behavior.
- Generated-reference fidelity and nested-member usability.
- Snippet presentation and failure diagnostics.
- Search result quality and canonical links.
- Version switching and explicit missing-version behavior.
- BCS and bridge scope boundaries.
- The absence of silent redirects or speculative routes.

Approval confirms that the architecture proof is suitable for scaling. Requested structural changes should be resolved before large-scale content work begins.

## Phase 5: parallel content expansion and hardening

After Human Gate 2, parallelize:

- Authored content expansion.
- Additional package declaration and nested-member rendering cases.
- CLI command coverage and parser fixtures.
- Search ranking and filtering.
- Accessibility and keyboard testing.
- Responsive and cross-browser testing.
- Build performance and database-fetch efficiency.
- Link, anchor, redirect, and 404 coverage.
- CI caching and actionable failure output.
- Operational documentation for database population and deployment.

Bridge subpage design, CLI example execution, bridge example execution, additional compiler APIs, and the WASM runner remain outside this phase.

## Phase 6: release data and launch candidate

1. Rotate any database credential that has previously been exposed and configure only the replacement secret in approved local, preview, and production environments.
2. Build or obtain the exact current canary and nightly CLI binaries.
3. Run dry-run generation and validation separately for each exact binary.
4. Review the resolved versions, source commits, package counts, projected-page counts, CLI command counts, hashes, and intended pointer changes.
5. Publish both complete exact-version releases through the operator-run population command.
6. Set the `canary` and `nightly` pointers only after their respective releases are complete.
7. Do not create a `stable` pointer until a stable release is intentionally published.
8. Build the production candidate from PlanetScale and deploy it as a noindex preview.
9. Run the complete CI suite and production smoke-test checklist against that candidate.

## Human gate 3: production launch approval

A human confirms:

- Canary and nightly resolve to the intended exact immutable releases.
- Neither snapshot is described as stable.
- Package and CLI content visibly match their selected compiled binaries.
- Secrets are absent from source, logs, generated HTML, and client bundles.
- Ordinary production pages do not require live browser-time database queries.
- Navigation, search, syntax highlighting, accessibility, and responsive behavior pass the launch checklist.
- `developer.boundaryml.com` and the permanent `docs.boundaryml.com` redirect are ready to activate.

Only after approval should production deployment and redirects proceed.

## Phase 7: production deployment

1. Run required checks from a clean revision.
2. Build one production artifact from the approved generated-content state.
3. Deploy atomically to `developer.boundaryml.com`.
4. Activate the permanent redirect from `docs.boundaryml.com`.
5. Smoke-test navigation, search, themes, generated reference, snippets, changelog, 404 behavior, and redirects.
6. Record the deployed source revision, selected projection version, generated-content versions, and deployment identifier.

## Suggested change sequence

The work may be organized as stacked or independent pull requests according to the repository's preferred review workflow:

| Change | Dependency | Parallelizable after dependency? |
|---|---|---|
| 0. Application scaffold and preview skeleton | None | No; establishes the shared base. |
| 1. Database migration, validators, routes, hashes, and sample payloads | Change 0 | No; establishes shared contracts. |
| 2A. Documentation shell and static routes | Human Gate 1 | Yes. |
| 2B. Snippet components and validation | Human Gate 1 | Yes. |
| 2C. Package generator and projection | Human Gate 1 | Yes. |
| 2D. CLI capture and parser | Human Gate 1 | Yes. |
| 2E. Authored content and changelog loaders | Human Gate 1 | Yes. |
| 2F. CI and preview expansion | Human Gate 1 | Yes, with shared-script coordination. |
| 3. Unified publisher and Postgres-backed integration | Changes 2C and 2D; consumes the others | No; convergence point. |
| 4. Complete architecture proof | Change 3 | No; review checkpoint. |
| 5. Content expansion and hardening | Human Gate 2 | Yes. |
| 6. Canary/nightly population and launch candidate | Required Change 5 work | No; release operation. |
| 7. Production deployment and redirects | Human Gate 3 | No; launch operation. |

## Human decisions that are already closed

Implementation should not stop to ask again about:

- The `typescript2/app-developer-docs` location.
- The initial standard-package allowlist.
- PlanetScale Postgres as the generated-content store.
- The five-table database layout.
- The absence of release publication statuses and stored page parents.
- FQN routes without `kind` and owner-page anchors for members.
- Direct rendering from one CLI artifact rather than CLI projection rows.
- Canary and nightly as the initial population, with no initial stable pointer.
- A one-time shadcn adaptation without upstream synchronization.
- BCS as a coming-soon page.
- Deferred bridge subpage taxonomy.
- Deferred WASM execution.
- No initial CLI or bridge example testing.
- No new compiler or public CLI documentation API.
- No filesystem generated-content mode.

Implementation should request new human direction only when evidence requires a material change to these decisions or when a missing product choice would substantially alter user-visible behavior or an irreversible production contract.

## Definition of done

The initial implementation is complete when:

- All three human gates have been approved.
- The portal builds and deploys from the agreed application path.
- The route contract and contextual 404 behavior match the proposal.
- Authored, structured, changelog, package, CLI, and snippet sources preserve their assigned ownership boundaries.
- Every displayed BAML snippet produces its declared result with the selected compiled CLI binary.
- A proper multi-file project is checked without synthesized project structure.
- The operator-run population command can dry-run, publish, inspect, and verify an exact release outside CI/CD.
- The portal has no filesystem generated-content reader or persistent generated release bundle.
- Package and CLI release data is complete, immutable, hash-verified, and published transactionally.
- Canary and nightly exact releases and pointers are present; stable is absent until intentionally published.
- Production pages are statically built from validated Postgres content.
- Search spans authored and generated content and links nested members to canonical owner-page anchors.
- CI validates content, snippets, generated records, routes, links, and the complete production build.
- The production deployment passes smoke tests and contains no exposed credentials.
- No repository-root `docs/` or `fern/` content participates in the application or pipeline.
