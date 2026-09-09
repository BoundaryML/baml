# Validation

## Compiler identity

Built the CLI from the unchanged pinned canary source `5b398f2b60cfa78258ac9534e8322d056564e85f` on macOS ARM64 using the repository-pinned Rust 1.98.0:

```sh
cd baml_language
cargo build --locked -p baml_cli --bin baml-cli
```

The debug build succeeded. The macOS linker emitted a compact-unwind size warning; no source change was needed. Binary SHA-256: `522ae68d51348962af44060fb80a79211c31ce126cba7ee10100070d7c23d1f1`. Its version string is still `baml-cli 0.18.0`, because the source release metadata has not been bumped. This is a source-identity validation of the proposed 0.19.0 cut, not a test of a published 0.19.0 binary.

The comparison binary is the published 0.18.0 macOS ARM64 release used in the backtest. This separates source changes from PR descriptions that accidentally describe behavior already present in the previous release.

## Examples and migrations

All 16 positive BAML code blocks in the final blog pass `check` against the candidate. All 13 BAML “before” blocks pass `check` with the published 0.18.0 CLI. Each snippet was isolated in a temporary project with a minimal `baml.toml` and one `baml_src/main.baml`. Checks set `BAML_AGENT_SKILL_CHECK=off` so the separate skill policy does not mask language diagnostics.

| Effect | Positive candidate check | Before on 0.18.0 | Before on candidate |
| --- | --- | --- | --- |
| C01: Call and parse bound LLM specifications | Pass | — | — |
| C02: More float math and range constants | Pass | — | — |
| C03: Infer lambda arguments through a union | Pass | — | — |
| C10: Use @ projections in BAML source | Pass | Pass | Rejected |
| C11: Rename the request client override | Pass | Pass | Rejected |
| C13: Make shared union members explicit | Pass | Pass | Rejected |
| C16: Use shorter standard-library type names | Pass | Pass | Rejected |
| C17: Use public parsing and testing APIs | Pass | Pass | Rejected |
| C18: Keep panics out of throws clauses | Pass | Pass | Still compiles; behavioral migration |
| C19: Handle the updated I/O error contracts | Pass | Pass | Rejected |
| C20: Remove obsolete catchable runtime error types | Pass | Pass | Rejected |
| C21: Check float division results explicitly | Pass | Pass | Still compiles; behavioral migration |
| C22: Update integer overflow handling | Pass | Pass | Still compiles; behavioral migration |
| C23: Omit the time argument to request midnight | Pass | Pass | Rejected |
| C24: Use Compare and Ordering for sorting | Pass | Pass | Rejected |
| C25: Use is_nan instead of non-reflexive equality | Pass | Pass | Still compiles; behavioral migration |

Runtime checks use the candidate CLI, without paid provider requests. The request-preview example only builds an HTTP request; it does not send it. The streaming example is compile-checked, not executed against an LLM.

| Runtime probe | Published 0.18.0 | Candidate |
| --- | --- | --- |
| Early closure return | E0001 against enclosing return type | `"early"` |
| Discarded conditional mutation and unused division | `false` | `true` |
| Float division by zero | `"panic"` | `"Infinity"` |
| Failed assertion panic class | `"UserPanic"` | `"AssertionFailed"` |
| NaN reflexivity and ordering above infinity | `false` | `true` |
| Issue #4468 array/map/loop | `5` | `5` — already fixed |
| Issue #4750 mixed signed-zero literals | `"0.0,0.0"` | `"0.0,0.0"` — still broken |
| Bound spec parsing (C01) | — | `Answer { value: 42 }` |
| Float methods/constants (C02) | — | `true` |
| Contextual callable-union lambda (C03) | — | `"HELLO"` |
| Named request client override (C11) | — | Correct POST request shape, no network call |
| Explicit panic catch (C18) | — | `0` |
| Ordering-based sort callback (C24) | — | `[1, 2, 3]` |
| is_nan migration (C25) | — | `true` |

The compiler probe for the discarded-expression regression used the CLI’s default optimization configuration. The PR’s own regression suite additionally covers O0/O1/O2; that full suite was not rerun for this documentation change.

## Agent skill policy

A temporary project without an installed skill failed `check` under `BAML_AGENT_SKILL_CHECK=require` with exit 4. `agent install` succeeded under the same policy. `check` then passed. The installed skill is stamped with the candidate binary’s current canonical version; publication will supply 0.19.0 metadata. The installation only modified the temporary test project.

## Claims corrected by differential checking

- Removed the proposed migration for inline `unreflect` escaping through a stored spec: 0.18.0 already rejects it with E0168.
- Removed the proposed migration for inference holes in required interface signatures: 0.18.0 already rejects it with E0147.
- Used `baml.panics.AssertionFailed`, the name in the final source, despite #4725’s prose saying `AssertionFailure`.
- Preserved #4751’s final accuracy qualifications. Its earlier “within one ulp” and correctly-rounded claims were withdrawn.
- Kept #4759’s performance evidence to static instruction counts; no runtime speedup claim.
- Did not notify #4750 as fixed, or reannounce #4468/#4587 as new fixes.
- Did not claim that #4742 removed access to release notes: #4743 and #4783 establish the final blog and redirects.

## Document and link checks

The inventory contains 74 PRs, 65 non-v0 followup candidates, 25 retained changelog PRs, and 34 effects: 9 features, 15 breaking changes, and 10 bug fixes. There are no headlines. All effect groups contain at most three PRs. Every syntax/library feature has a code block, every breaking entry has before/after blocks, and the performance-only feature has its source measurements.

The post frontmatter now sets `isPublished: true` for review in the PR’s website preview. Website Git deployments are enabled for all branches in `typescript2/app-website/vercel.json`. Merging the post into `canary` also makes it eligible for production publication. No tag, package version, or user notification was changed.

The developer portal, package catalog, and CLI catalog returned HTTP 200. Both product and developer `/changelog` URLs returned 308 to the product blog’s release filter, which returned 200. Relative artifact links, PR coverage, Markdown fences, and frontmatter were checked locally.

The final source-linked Markdown was checked with `git diff --check` and the applicable `prek` hooks. Browser/VS Code source-navigation smoke tests, live workerd execution, Linux installer containers, SDK runtime matrices, and a final published-0.19.0 smoke test were not rerun; the draft attributes those changes to their merged PRs rather than claiming local end-to-end coverage.
