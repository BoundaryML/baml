# Developer Documentation Implementation Status

## Purpose and authority

This document is the execution ledger for [DEVELOPER_DOCS_IMPLEMENTATION_PLAN.md](./DEVELOPER_DOCS_IMPLEMENTATION_PLAN.md). It records current progress, verification evidence, human-gate state, implementation notes, and any deviations discovered while doing the work.

The requested implementation-provenance audit is maintained in [DEVELOPER_DOCS_SHADCN_COPY_REPORT.md](./DEVELOPER_DOCS_SHADCN_COPY_REPORT.md). It reports copied versus non-copied shell percentages without treating visual similarity as copied code.

The authority order is unchanged:

1. [DEVELOPER_DOCS_PROPOSAL.md](./DEVELOPER_DOCS_PROPOSAL.md) defines product scope, routes, content ownership, design direction, and storage semantics.
2. [DEVELOPER_DOCS_COMPILER_ENABLEMENT.md](./DEVELOPER_DOCS_COMPILER_ENABLEMENT.md) defines the boundary between the portal and BAML tooling.
3. [DEVELOPER_DOCS_IMPLEMENTATION_PLAN.md](./DEVELOPER_DOCS_IMPLEMENTATION_PLAN.md) defines execution order, checkpoints, and human gates.
4. This status document reports execution. It cannot silently override the other three documents.

If implementation evidence requires a material change, record it under **Deviations and blockers** and obtain the required human direction before changing a closed decision or irreversible contract.

## Permanent safety constraints

- Never read, search, import, copy, or infer behavior from the repository-root `docs/` or `fern/` directories. They are out of date and are not inputs.
- Do not add public compiler or CLI documentation APIs, a file-checking command, a diagnostic protocol, or a documentation SDK.
- Use one explicitly selected compiled `baml` binary for all package, help, version, and same-workflow snippet invocations.
- Use Postgres as the portal's generated-content source in every environment; do not add a filesystem-backed reader or persistent generated release bundle.
- Keep credentials in environment variables only and never print or persist them.
- Preserve exact-version immutability and move a channel pointer only after its complete release exists.
- Do not add definition-kind URL segments, member routes, stored page parents, release statuses, a CLI projection table, integrations, speculative BCS or bridge routes, or the WASM runner to the launch path.

## Current snapshot

- **Current phase:** Human Gate 1 checkpoint 1 is complete; checkpoint 2 preparation is active.
- **Current gate:** Checkpoint 2 — stored payload and rendering approval — is unblocked but not yet complete.
- **Live database mutations:** Approved checkpoint-1 creation and migration only: `boundaryml/developer-docs/development` now contains the five empty generated-content tables. No generated rows or channel pointers were written.
- **Generated-content publication:** None.
- **Last updated:** 2026-09-03.

## Progress by phase

### Phase 0 — preflight and scaffold

- [x] Confirm application path: `typescript2/app-developer-docs`.
- [x] Read the proposal, compiler-enablement supplement, and implementation plan completely.
- [x] Confirm the local shadcn v4 reference path and pinned commit.
- [x] Inspect relevant `typescript2` workspace conventions without accessing legacy documentation inputs.
- [x] Finalize compatible framework and tooling versions.
- [x] Create the application directories and declared dependencies.
- [x] Add lint, type-check, test, and production-build commands.
- [x] Add the minimal GitHub Actions check and noindex Vercel preview behavior.
- [x] Add the one-time documentation-shell adaptation and MIT notice.
- [x] Add generated-cache, secret, and temporary-file guardrails.
- [x] Verify the empty portal builds in the workspace.

### Phase 1 — contracts and database tooling skeleton

- [x] Add the reviewed five-table SQL migration.
- [x] Add database-row and payload validators.
- [x] Declare the initial `PAGE_SCHEMA_VERSION`.
- [x] Add canonical JSON and SHA-256 utilities.
- [x] Add FQN route and deterministic member-anchor utilities.
- [x] Add the build-time Postgres reader.
- [x] Add migration, population, inspection, and verification command skeletons.
- [x] Generate representative package and CLI samples in memory from one real compiled binary.
- [x] Validate representative payloads without persisting a generated-content bundle.

### Human Gate 1 — stored and rendered contract approval

- **Checkpoint 1, target and migration:** Approved and completed on 2026-09-03.
- **Checkpoint 2, payload and rendering:** Complete in-memory generation now passes with the approved temporary `boundary.id` namespace-page exception. Live population and Postgres-backed rendering review remain pending.
- No population or channel-pointer update is authorized by checkpoint 1 approval. Checkpoint 2 remains open.

#### Checkpoint 1 review package

The approved and applied isolated development target is:

| Field | Proposed value | Evidence and constraint |
|---|---|---|
| Organization | `boundaryml` | The authenticated CLI can enumerate this organization. |
| Database | `developer-docs` | Dedicated PostgreSQL database, created 2026-09-03 in AWS `us-east`. |
| Branch | `development` | Isolated non-production `PS-DEV` branch, created from the empty default branch and ready. |
| Canonical migration | [`typescript2/app-developer-docs/migrations/0001-generated-content.sql`](./typescript2/app-developer-docs/migrations/0001-generated-content.sql) | Exact five-table proposal schema. |
| Migration SHA-256 | `f2cbc7f5cd2b1f062458a2ff9df57d03e21ae753621689638821e14225d7b915` | Recomputed by the review-only migration command. |

Initial read-only discovery found only `boundaryml/ops-product-metrics/main`; its `main` branch is production. That existing database remains explicitly excluded from this project.

The user approved this checkpoint on 2026-09-03 by replying “I'm good to check off!” to the explicit target-and-migration approval request. PlanetScale created its ordinary default `main` branch along with the database; no schema was applied there, and a read-only catalog check confirmed it has zero `developer_docs` tables. The reviewed migration was applied only to `development` through PlanetScale's PostgreSQL shell. No connection string or password was printed or persisted.

Read-only post-migration verification found exactly the five expected tables, all empty. PlanetScale's branch-schema output and `pg_catalog` confirmed their primary keys, unique constraints, foreign keys with `ON DELETE RESTRICT`, check constraints, identity column, data types, and defaults.

### Corrective shell pass requested during review

- [x] Re-read the proposal, implementation plan, and compiler-enablement supplement without accessing the repository-root `docs/` or `fern/` directories.
- [x] Re-audit the pinned shadcn v4 reference at commit `63c1308d112b6b1205d86244a156cca1abef5087`.
- [x] Replace inert header labels, fake search, fake mobile control, and the homepage documentation columns with directly copied-and-adapted shadcn shell structure.
- [x] Perform a second source-level port after visual review: remove conflicting Fumadocs shell CSS and copy the shadcn header, navigation, docs grid, 288px rails, 640px article, Typeset rules, page actions, theme, homepage hero/card rail, and mobile behavior.
- [x] Add working desktop navigation, mobile navigation, search command menu, active states, copy-page and previous/next controls, right-hand page contents, theme switching, and footer behavior.
- [x] Add useful static roots for BAML, CLI, Boundary Cloud Services, tutorials, examples, and changelog plus the initial BAML subsection roots.
- [x] Correct the formal cloud product name to **Boundary Cloud Services** in the portal and proposal.
- [x] Add and browser-verify a branded custom 404 with working recovery links; do not expose the default Vercel/framework 404.
- [x] Add the requested shadcn copy-provenance report with copied and non-copied percentages.
- [x] Add representative authored subpages for a book part, complete book chapter, language-reference leaf, TypeScript bridge, tutorial, and focused example so the shell can be reviewed as a real hierarchy.
- [x] Add visible nested breadcrumbs, deepest-item sidebar selection, styled callouts and tables, and labeled bottom previous/next navigation for subpage review.
- [x] Replace flat depth styling with a recursive, collapsible navigation tree that preserves links and open state across desktop and mobile surfaces.
- [x] Complete final automated and browser verification for this pass.

### Later phases

- Phase 2 workstreams: The documentation shell/static-route portion and representative authored page shapes received an explicit user-directed corrective pass before database approval; generated content and all remaining workstreams remain blocked on Human Gate 1.
- Phase 3 integration: Not started.
- Phase 4 architecture proof: Not started.
- Human Gate 2: Not started.
- Phase 5 expansion and hardening: Not started.
- Phase 6 launch candidate: Not started.
- Human Gate 3: Not started.
- Phase 7 production deployment: Not started.

## Preflight evidence and decisions

### Repository and tooling

- The `typescript2` workspace uses pnpm 11.1.3, Turbo 2.8.9, and top-level `app-*` packages.
- The existing workspace Next.js application originally established Next.js 15.5.9 and React 19.2.3 as compatible local baselines. With explicit user approval, the developer-docs application was subsequently upgraded in place to the validated Next.js 16 stack recorded below; sibling applications were not upgraded.
- The local shadcn v4 reference exists at `/Users/vbv/repos/reference/shadcn-ui/apps/v4` and is exactly at commit `63c1308d112b6b1205d86244a156cca1abef5087`, matching the proposal.
- The reference is MIT licensed. Any substantially adapted shell code requires the notice in `typescript2/app-developer-docs/THIRD_PARTY_NOTICES.md`.
- A compiled `baml` binary is available at `/opt/homebrew/bin/baml`. Preflight output reports wrapper `0.2.4` and toolchain `0.18.1-nightly.20260828.a`. It is a candidate for representative contract generation, not an approved canary/nightly publication binary.

### Dependency compatibility notes

- The initial scaffold used Next.js 15.5.9, Fumadocs core/UI 15.8.5, and Fumadocs MDX 12.0.3. That tuple was a valid compatibility fallback after Fumadocs MDX 14.3.2 failed at install time by importing a core 16-only module.
- With explicit user approval on 2026-09-03, the application was upgraded to Next.js 16.3.4, React and React DOM 19.2.8, Fumadocs core/UI 16.15.4, and Fumadocs MDX 15.4.0. These are stable releases newer than the pinned shadcn reference's Next.js 16.3.0 canary, Fumadocs core/UI 16.10.5, Fumadocs MDX 15.0.12, and React 19.2.3 tuple.
- A direct post-upgrade audit found that the Fumadocs APIs used by the pinned shadcn shell were also exported by the former core 15.8.5 and MDX 12.0.3 packages. The earlier version constraint prevented copying the reference dependency tuple verbatim, but it did not require the shell-level navigation, search, breadcrumb, TOC, primitive-component, or route-architecture differences. Those differences are local product, dependency-surface, or static-export decisions.
- The workspace continues to override React type packages to `@types/react` 18.3.26 and `@types/react-dom` 18.3.7 for sibling-package compatibility. The developer-docs application passes type-check with that shared override; a workspace-wide React 19 type migration is outside this portal upgrade.
- Tailwind CSS remains 4.1.11 rather than the reference's 4.3.x line. Direct compilation probes confirmed that the distinctive reference utilities used by the shell are accepted by 4.1.11, so no audited shell difference is attributable to that version gap.
- The remaining non-verbatim shadcn areas are documented in `DEVELOPER_DOCS_SHADCN_COPY_REPORT.md`. In particular, the reference's fetch-backed search endpoint cannot be copied literally while this application retains `output: 'export'`; that is a deployment-architecture constraint, not a framework-version constraint.
- Final dependency selections are recorded only after installation, peer-dependency validation, type-check, tests, and production build succeed.

## Deviations and blockers

The following user-directed sequencing exception is accepted for the shell review:

```text
Date: 2026-09-02
Plan/proposal requirement: Phase 2 documentation-shell work normally begins after Human Gate 1.
Observed evidence: Human review found that the Phase 0 shell was not a faithful shadcn adaptation and its homepage navigation, search, and mobile controls were inert.
Impact: The portal could not be meaningfully reviewed before the database checkpoint.
Proposed deviation or resolution: Complete the shadcn-shell, static-root, and explicitly requested representative authored-subpage corrective pass before resuming Human Gate 1, while keeping every database mutation and generated-content publication stopped.
Approval required: Explicitly requested by the human reviewer on 2026-09-02.
Status: Accepted for documentation-shell, static-route, and representative authored-subpage work only.
```

The following compiler condition and user-approved temporary portal exception are recorded:

```text
Date: 2026-09-01
Plan/proposal requirement: Package routes come directly from qualified-name segments; kind segments and other collision workarounds are forbidden, and a complete allowlisted release must be validated before publication.
Observed evidence: Candidate binaries through 0.18.1-nightly.20260901.a export both top-level function V:boundary.id (qualified name boundary.id) and namespace-owned function V:boundary.id.current with namespace ["id"] and name "current". Under the approved projection, packages, namespaces, and top-level classes, enums, interfaces, aliases, and functions are routable; nested members and implementations are anchor-only. The namespace page boundary.id and top-level function boundary.id are therefore both legitimately routable and both require route_path boundary/id. Compiler source currently has an explicit boundary.id-only exception in crates/baml_compiler2_hir/src/package.rs that suppresses the otherwise detected namespace-shadow diagnostic.
Impact: Without an explicit exception, the generator correctly rejects the boundary package with "Projected package route collision: boundary/id." The user identified the underlying namespace shadow as a compiler bug and will fix it outside this documentation work.
Proposed deviation or resolution: Temporarily suppress only the synthesized boundary.id namespace landing page. Preserve the top-level boundary.id function page and boundary.id.current function page. Keep ordinary namespace projection and collision failures unchanged for every other package and qualified name. Remove or reassess this exception after the compiler no longer exports the shadow.
Approval required: Explicitly approved by the user on 2026-09-03.
Status: Accepted temporary portal exception. The full 13-package dry run now passes; the compiler bug remains externally owned.
```

Use this format for future entries:

```text
Date:
Plan/proposal requirement:
Observed evidence:
Impact:
Proposed deviation or resolution:
Approval required:
Status:
```

## Verification log

| Date | Scope | Command or evidence | Result |
|---|---|---|---|
| 2026-09-01 | Specification review | Complete line-by-line read of the 1,286-line proposal, 463-line plan, and 372-line compiler supplement | Passed |
| 2026-09-01 | Workspace preflight | Inspected `typescript2` package, workspace, Turbo, and relevant application configuration | Passed |
| 2026-09-01 | Design baseline | Verified local shadcn v4 reference commit and MIT license | Passed |
| 2026-09-01 | Compiled CLI availability | `/opt/homebrew/bin/baml --version` | Candidate binary available |
| 2026-09-01 | Dependency compatibility | Fumadocs MDX 14.3.2 postinstall against core 15.8.5 | Failed; corrected to the core 15-compatible MDX 12.0.3 line |
| 2026-09-01 | Dependency peers | `pnpm peers check --filter app-developer-docs` | Passed |
| 2026-09-01 | Phase 0 quality checks | Portal lint, Node tests, and TypeScript type-check | Passed |
| 2026-09-01 | Phase 0 preview build | `VERCEL_ENV=preview pnpm --filter app-developer-docs build` | Passed; fully static export |
| 2026-09-01 | Preview indexing guard | Checked exported `out/robots.txt` for `Disallow: /` | Passed |
| 2026-09-01 | Canonical migration | Compared `0001-generated-content.sql` with the proposal's five-table DDL and ran the explicit-target review-only command | Passed; SHA-256 `f2cbc7f5…b915`, no writes |
| 2026-09-01 | Representative package contract | Generated and validated `baml` export format 1 and 470 projected pages entirely in memory | Passed; raw export SHA-256 `28923afc…c424` |
| 2026-09-01 | Representative CLI contract | Recursively captured 31 help entries and 19 root commands from the same binary | Passed; source SHA-256 `fab6e71b…e44d`, payload SHA-256 `a0879928…088` |
| 2026-09-01 | Allowlist projection | Validated 12 of 13 packages individually: `baml`, `reflect`, `testing`, `assert`, `log`, `ai`, `openai`, `anthropic`, `google`, `aws`, `vercel`, and `claude_code` | Passed |
| 2026-09-01 | Complete-release dry run | Generated all allowlisted inputs from candidate binary `0.18.1-nightly.20260828.a` | Correctly blocked on the `boundary/id` collision; no writes |
| 2026-09-01 | PlanetScale discovery | Authenticated read-only inventory of `boundaryml` databases and branches | Only unrelated production `ops-product-metrics/main` exists; excluded |
| 2026-09-01 | Phase 1 quality checks | Portal lint, 10 Node tests, TypeScript type-check, and preview production build after contract implementation | Passed |
| 2026-09-02 | Corrective specification review | Re-read the proposal, implementation plan, and compiler supplement; re-audited the pinned shadcn v4 shell implementation | Passed; repository-root `docs/` and `fern/` remained excluded |
| 2026-09-02 | Shell interaction | Browser-tested desktop navigation, active states, copy-page, previous/next, search filtering/navigation, theme switching, cards, and table-of-contents links | Passed; no browser console errors |
| 2026-09-02 | Live shadcn comparison | Opened `ui.shadcn.com/docs` and the local BAML page side by side, then measured both rendered shells at 1280px | Exact match on the 64px header, 288px sidebar, 640px article, 288px TOC, article x/y position, 15px/22.5px text rhythm, 30px/36px title, and rail placement |
| 2026-09-02 | Mobile shell | Emulated a true 390×844 viewport and inspected computed layout | Passed; 390px document width with no horizontal overflow, Menu/GitHub/theme controls fit, and sidebar/TOC/page actions hide as in the reference |
| 2026-09-02 | Static routes | Requested every visible shell destination from the clean development server | 12 expected routes returned 200 |
| 2026-09-02 | Contextual 404s | Requested an unversioned BAML release path, speculative BCS and bridge paths, and an unknown CLI version | All returned 404 |
| 2026-09-02 | Custom 404 presentation | Browser-opened an unknown route and inspected the rendered page and recovery links | Passed; branded portal 404 rendered inside the shared shell, not the default Vercel page |
| 2026-09-02 | Initial shadcn provenance audit | Block-by-block copied/adapted audit plus strict normalized-line measurement before the requested subpage hierarchy | Snapshot recorded in the copy report history; superseded by the current tree-aware measurement |
| 2026-09-02 | Navigation, geometry, and 404 regression tests | Added route-existence, Boundary Cloud Services naming, branded 404, and measured shadcn geometry checks | Passed; full suite now 14 tests |
| 2026-09-02 | Corrective quality checks | Portal lint, peer checks, TypeScript type-check, 14 tests, and preview production build after the direct shell port | Passed; build exported 17 static pages and the clean development server was restarted |
| 2026-09-02 | Representative subpages | Browser-reviewed the complete functions chapter with four-level breadcrumbs, nested sidebar selection, article/TOC rails, code blocks, callout, and labeled page navigation | Passed at 1280px; article remains in the measured 640px shell and has no horizontal overflow |
| 2026-09-02 | Subpage route contract | Requested all 18 visible shell destinations plus unknown BAML-version, deeper bridge, deeper BCS, and unauthored example paths | All visible routes returned 200; all unsupported routes returned 404 |
| 2026-09-02 | Subpage quality checks | Portal lint, TypeScript type-check, 14-test suite, diff check, and preview production build after adding six authored nested routes | Passed; build exported 23 static pages and the development server was restarted |
| 2026-09-02 | Collapsible hierarchy | Browser-tested first- and second-level disclosure controls, persistent user state, automatic active-ancestor expansion, deepest-page selection, nested width calculation, and full-rail overflow | Passed; second-level content ends exactly at the 232px rail boundary with zero document overflow |
| 2026-09-02 | Tree regression checks | Portal lint, TypeScript type-check, and nested route ancestry/uniqueness tests | Passed; full suite now 15 tests |
| 2026-09-02 | Current shadcn provenance audit | Recalculated after the local recursive tree implementation | Shell: 49.7% directly copied/adapted and 50.3% not copied; strict near-verbatim measurement: 28.5% / 71.5% |
| 2026-09-03 | Human Gate 1 checkpoint 1 | User approved the explicitly proposed `boundaryml/developer-docs/development` target and migration `f2cbc7f5…b915` | Approved |
| 2026-09-03 | PlanetScale target creation | Created dedicated PostgreSQL database `developer-docs` and non-production `PS-DEV` branch `development` in `boundaryml` | Passed; both resources reached ready state |
| 2026-09-03 | Development migration | Applied unchanged `0001-generated-content.sql` through the supported PlanetScale PostgreSQL shell | Passed; one schema and five tables created |
| 2026-09-03 | Applied-schema verification | Queried table inventory, `pg_catalog` constraints, PlanetScale branch schema, and all five row counts | Passed; exact five-table shape present and all tables empty |
| 2026-09-03 | Default-branch isolation | Queried `developer-docs/main` after migration | Passed; zero `developer_docs` tables on `main` |
| 2026-09-03 | `boundary/id` root cause | Rechecked nightly `0.18.1-nightly.20260901.a`, the approved routability rules, builtin sources, and compiler namespace-shadow detection | Confirmed compiler bug; portal projection is correct, compiler fix is externally owned |
| 2026-09-03 | Temporary namespace-page exception | Added exact-match suppression for only the synthesized `boundary.id` namespace page plus a negative-control regression test | Passed; `boundary.id` function and `boundary.id.current` function remain routed, all other collisions still fail |
| 2026-09-03 | Complete-release dry run | Generated all 13 allowlisted packages and recursive CLI payload from exact nightly `0.18.1-nightly.20260901.a` | Passed; 1,496 projected pages, 37 CLI commands, no writes |
| 2026-09-03 | Temporary-exception quality checks | Portal lint, TypeScript type-check, 17-test suite, and preview production build | Passed; build generated 26 static pages |
| 2026-09-03 | Framework dependency upgrade | Upgraded only `app-developer-docs` to Next.js 16.3.4, React 19.2.8, Fumadocs core/UI 16.15.4, and Fumadocs MDX 15.4.0 | Passed installation and peer-dependency validation without changing sibling application versions |
| 2026-09-03 | Next.js 16 quality checks | Portal lint, TypeScript type-check, 16-test suite, and preview production build | Passed; build exported 26 static pages and preview `robots.txt` retained `Disallow: /` |
| 2026-09-03 | Next.js 16 browser regression | Rechecked desktop shell, search navigation, theme switching, active navigation, TOC, overflow, and browser console | Passed; no console warnings or errors |
| 2026-09-03 | shadcn version-constraint audit | Compared every Fumadocs API imported by the pinned reference with both the former and current installed packages, and compiled the reference's distinctive Tailwind utility forms with 4.1.11 | No shell-level difference was forced by the former Next.js/Fumadocs/Tailwind versions; remaining differences are architectural or product-specific |

## Human gate record

| Gate | State | Approval evidence | Notes |
|---|---|---|---|
| Human Gate 1 | Checkpoint 1 complete; checkpoint 2 in progress | User reply “I'm good to check off!” on 2026-09-03 | Migration applied and verified only on `boundaryml/developer-docs/development`; complete generation now passes, but no population or pointer update has occurred. |
| Human Gate 2 | Not started | None | Requires completed architecture-proof preview. |
| Human Gate 3 | Not started | None | Requires completed launch candidate. |
