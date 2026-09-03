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

- **Current phase:** Corrective documentation-shell review after Phase 1; database work remains stopped at Human Gate 1.
- **Current gate:** Checkpoint 1 — exact target and migration — remains unapproved.
- **Live database mutations:** None.
- **Generated-content publication:** None.
- **Last updated:** 2026-09-02.

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

- **Checkpoint 1, target and migration:** Ready for review; approval has not been granted.
- **Checkpoint 2, payload and rendering:** Prepared with representative in-memory payloads, but blocked on checkpoint 1 by plan order and on a real collision in the candidate `boundary` export described below.
- No PlanetScale branch/database creation, migration, or population is authorized before checkpoint 1 approval identifies the exact target.

#### Checkpoint 1 review package

The proposed isolated development target is:

| Field | Proposed value | Evidence and constraint |
|---|---|---|
| Organization | `boundaryml` | The authenticated CLI can enumerate this organization. |
| Database | `developer-docs` | New dedicated PostgreSQL database; it does not currently exist. |
| Branch | `development` | New isolated non-production branch; it does not currently exist. |
| Canonical migration | [`typescript2/app-developer-docs/migrations/0001-generated-content.sql`](./typescript2/app-developer-docs/migrations/0001-generated-content.sql) | Exact five-table proposal schema. |
| Migration SHA-256 | `f2cbc7f5cd2b1f062458a2ff9df57d03e21ae753621689638821e14225d7b915` | Recomputed by the review-only migration command. |

Read-only discovery found only `boundaryml/ops-product-metrics/main`; its `main` branch is production. That existing database is explicitly excluded from this project and was not selected merely because it already exists.

Approval of this checkpoint authorizes only the normal next actions in the implementation plan: create the exact dedicated PostgreSQL target above if still absent, create its isolated development branch if still absent, acquire credentials through the supported PostgreSQL role workflow without logging or persisting them, and apply the reviewed migration to that development branch. It does not authorize production work, destructive SQL, population, or a channel-pointer update.

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
- The existing workspace Next.js application establishes Next.js 15.5.9 and React 19.2.3 as compatible local baselines.
- The local shadcn v4 reference exists at `/Users/vbv/repos/reference/shadcn-ui/apps/v4` and is exactly at commit `63c1308d112b6b1205d86244a156cca1abef5087`, matching the proposal.
- The reference is MIT licensed. Any substantially adapted shell code requires the notice in `typescript2/app-developer-docs/THIRD_PARTY_NOTICES.md`.
- A compiled `baml` binary is available at `/opt/homebrew/bin/baml`. Preflight output reports wrapper `0.2.4` and toolchain `0.18.1-nightly.20260828.a`. It is a candidate for representative contract generation, not an approved canary/nightly publication binary.

### Dependency compatibility notes

- The shadcn reference's Fumadocs 16.10.5 packages require Next.js 16 and therefore cannot be copied blindly into the Next.js 15.5.9 workspace baseline.
- The shell now uses the same Geist package and directly ported Typeset rules as the reference while retaining the workspace-compatible Next.js and Fumadocs versions.
- Fumadocs core/UI 15.8.5 declare Next.js 14 or 15 compatibility. Fumadocs MDX 14.3.2 advertised a broad core peer range but failed at install time because it imported a core 16-only module; the scaffold therefore uses MDX 12.0.3, whose declared peer range and release timing match core 15 and Next.js 15. This is a compatibility correction, not a specification deviation.
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

The following evidence-backed blocker remains open:

```text
Date: 2026-09-01
Plan/proposal requirement: Package routes come directly from qualified-name segments; kind segments and other collision workarounds are forbidden, and a complete allowlisted release must be validated before publication.
Observed evidence: Candidate binary 0.18.1-nightly.20260828.a exports both top-level function V:boundary.id (qualified name boundary.id) and namespace-owned function V:boundary.id.current with namespace ["id"] and name "current". The namespace page boundary.id and the top-level function boundary.id both require route_path boundary/id.
Impact: The generator correctly rejects the boundary package with "Projected package route collision: boundary/id." A complete 13-package release cannot be generated or populated from this candidate binary. The independent target-and-migration review is not blocked.
Proposed deviation or resolution: Do not change the portal route contract. Select the exact current canary/nightly publication binary and require it to pass the same collision check; if its export retains the collision, obtain an explicit compiler/product decision that gives the declarations distinct qualified names before checkpoint 2.
Approval required: Human direction is required if the release binary still contains the collision. Any proposal to change the closed route contract would require an explicit specification change.
Status: Open; blocks Human Gate 1 checkpoint 2 population, not checkpoint 1.
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

## Human gate record

| Gate | State | Approval evidence | Notes |
|---|---|---|---|
| Human Gate 1 | Awaiting checkpoint 1 approval | None | Proposed target is `boundaryml/developer-docs/development`; stop before target creation or live schema application. |
| Human Gate 2 | Not started | None | Requires completed architecture-proof preview. |
| Human Gate 3 | Not started | None | Requires completed launch candidate. |
