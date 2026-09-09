# Release followup drafts

Backtest only: nothing has been posted. These drafts refer to the already-published BAML 0.18.0 release. There are 17 destinations: nine external-authored PRs and eight external-authored issues. Seven distinct GitHub handles are represented; `ryanmazzolini` and `rmazzolini` are retained separately because account identity was not assumed.

All 110 non-v0 PRs were screened, including CI-only changes, reverted changes, and internal refactors. GitHub PR bodies, issue comments, reviews, and inline review comments were read. Directly referenced GitHub issues and their comments were read. Forty Linear issues were retrieved from release references and linked source context, including comments and attachments. Six linked Slack source threads were also read; they contain internal discussion or point back to the GitHub reports listed here.

The only Discord link in the original 114-PR inventory belongs to v0-only #4202. No Discord thread was linked from the in-scope PR/issue/Linear source records inspected. There is therefore no verified Discord destination to notify. This is a reference-graph audit, not a search of every Discord message for unlinked reports.

GitHub `MEMBER`, `OWNER`, and `COLLABORATOR` associations were treated as internal, and bot accounts were removed. `CONTRIBUTOR` and `NONE` human accounts were retained. This is the observed repository-association rule, not an employment directory. CI-only contributors receive contribution acknowledgments without inventing a user-facing release feature.

# GitHub PR: https://github.com/BoundaryML/baml/pull/4378

> Ruby needs a safe process-wide loader before the Ruby/Sorbet SDK can call the BAML 1.0 runtime.

```text
Thanks @ryanmazzolini and @rmazzolini for the private Ruby bridge loader and ABI validation groundwork. Your contribution is included in the changes leading to BAML 0.18.0. This is groundwork for Ruby support; it does not announce a complete callable Ruby SDK.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4499

> Timing-sensitive tests currently encode idle-16-vCPU behavior as correctness: the cancellation suites assert sub-second wall-clock bounds, the prof soak bounds a

```text
Thanks @harivansh-afk for machine-independent timing bounds and hermetic test setup. Your contribution is included in the changes leading to BAML 0.18.0. These are test-infrastructure changes included in the release range.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4479

> Two small changes that cost nothing on your runners and unbreak CI for anyone running this repo's workflows outside it.

```text
Thanks @harivansh-afk for the sccache log-path fix that respects TMPDIR. Your contribution is included in the changes leading to BAML 0.18.0. This acknowledgment is for the development and CI environment improvement.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4481

> The attw gate packs the working tree verbatim, so the tarball it checks carries the debug `.node` binary: **630MB, 99.7%

```text
Thanks @harivansh-afk for the faster TypeScript declaration gate that excludes the unused native binary. Your contribution is included in the changes leading to BAML 0.18.0. This improves our checks; it is not a runtime performance claim.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4537

> Copies of the Linux CI lanes pointed at ix machines, running side by side with the gating Blacksmith checks, plus

```text
Thanks @harivansh-afk for the Linux CI runner preview. Your contribution is included in the changes leading to BAML 0.18.0. This acknowledges the CI contribution included in the release range.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4539

> Nothing outside the `ix-*` files.

```text
Thanks @harivansh-afk for the Linux-only scope and cache isolation fixes for the CI preview. Your contribution is included in the changes leading to BAML 0.18.0. This acknowledges the CI contribution included in the release range.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4510

> Related to #4506 — discovered while investigating that compiler crash.

```text
Thanks @ritunjaym for the lazy take, skip, take_while, and skip_while iterator adapters. Your contribution is included in the changes leading to BAML 0.18.0. The separate unknown-method compiler defect in #4506 is still reproducible on 0.18.0; this release does not close that report.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4552

> Runner provisioning for the ix preview lanes now lives in the ix GitHub App it watches

```text
Thanks @harivansh-afk for the GitHub App integration for CI runner provisioning. Your contribution is included in the changes leading to BAML 0.18.0. This acknowledges the CI contribution included in the release range.

Release notes: https://boundaryml.com/changelog
```

# GitHub PR: https://github.com/BoundaryML/baml/pull/4558

> Nothing outside the `ix-*` workflow files.

```text
Thanks @harivansh-afk for shared compiler-cache support for the Linux CI runners. Your contribution is included in the changes leading to BAML 0.18.0. This acknowledges the CI contribution included in the release range.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4335

Source PRs: [#4526](https://github.com/BoundaryML/baml/pull/4526).

> rust codegen panics on `naming_convention = "language"` instead of emitting a diagnostic

```text
Thanks @BenSpex for the report. BAML 0.18.0 now reports an E0019 diagnostic for unsupported generator naming conventions instead of panicking. For Rust, use naming_convention = "preserve-case".

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4355

Source PRs: [#4502](https://github.com/BoundaryML/baml/pull/4502).

> addon fails to load on Alpine

```text
Thanks @tha-hammer for the report. BAML 0.18.0 includes the musl Node bridge build fix for x86_64 and aarch64. The PR verified that both addons link against musl and load on Alpine.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4376

Source PRs: [#4522](https://github.com/BoundaryML/baml/pull/4522).

> default output_dir writes OUTSIDE the project directory

```text
Thanks @BenSpex for the report. With BAML 0.18.0, an omitted output_dir generates baml_sdk next to baml.toml. Explicit output_dir values still select the parent directory for baml_sdk.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4421

Source PRs: [#4508](https://github.com/BoundaryML/baml/pull/4508), [#4544](https://github.com/BoundaryML/baml/pull/4544).

> for loop with multi-variable reassignment + array push hangs

```text
Thanks @tha-hammer for the report. BAML 0.18.0 includes the compiler fixes for reassigned locals and short-circuit expressions across loops. The reported multi-variable/array-push case is covered by the regression tests.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4468

Source PRs: [#4490](https://github.com/BoundaryML/baml/pull/4490).

> compiler ABORTS (SIGABRT) instead of reporting

```text
Thanks @briancripe for the report. BAML 0.18.0 fixes compilation of loops over joined Array.map results and Iterable-bounded generics. The inferred-result shape in this report is covered by the regression tests.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4589

Source PRs: [#4612](https://github.com/BoundaryML/baml/pull/4612).

> Only "optional field + omitted optional class-typed sub-key" loses the data.

```text
Thanks @BenSpex for the report. BAML 0.18.0 fixes the omitted-optional-member scoring issue. A valid optional object is retained when a nested optional member is omitted. The fix has deterministic regression coverage through both OpenAI Chat Completions and Responses.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4429

Status: partial followup. Source PRs: [#4409](https://github.com/BoundaryML/baml/pull/4409).

> produce **zero output**

```text
Thanks @tha-hammer for the report. BAML 0.18.0 addresses the baml run part of this report: log.* events appear with --log <LEVEL> or BAML_LOG. We verified the error-level example on the released 0.18.0 binary. Packed-binary logging and --log-file are not established as fixed by this change, so this is a partial update.

Release notes: https://boundaryml.com/changelog
```

# GitHub issue: https://github.com/BoundaryML/baml/issues/4588

Status: partial followup. Source PRs: [#4459](https://github.com/BoundaryML/baml/pull/4459), [#4583](https://github.com/BoundaryML/baml/pull/4583).

> All four scenarios now produce distinct, typed throws with a full traceback

```text
Thanks @BenSpex for the report. The runtime/error-boundary updates are now in BAML 0.18.0. Your confirmation on the August 24 nightly is consistent with a fresh 0.18.0 check: the dead-endpoint repro reports ai.errors.NetworkFailure with a traceback instead of the union dump. This does not resolve your separate stderr-content policy question. The release also requires ctx.output_format() and removes hash string literals; migration examples are in the changelog.

Release notes: https://boundaryml.com/changelog
```

# Screened reports with no release notification

| Source | Decision |
| --- | --- |
| [#4506](https://github.com/BoundaryML/baml/issues/4506) | Do not claim the compiler crash is fixed. The 0.18.0 binary still throws an internal compiler error for the inferred generic receiver. #4510 adds iterator adapters and explicitly disclaims fixing this report. |
| [#4371](https://github.com/BoundaryML/baml/issues/4371) | No in-range fix for Rust code generation of non-identifier literal union arms was established. |
| [#4422](https://github.com/BoundaryML/baml/issues/4422) | Multi-part documentation/DX report. It is referenced context, not a verified fixed issue in this release. |
| [#4279](https://github.com/BoundaryML/baml/issues/4279) | Musl packing fix predates the lower tag; distinct from #4355’s Node addon fix. |
| [#4154](https://github.com/BoundaryML/baml/issues/4154) | V0 provider metadata issue. Its thread already records release 0.224.0 as the fix. |
| [#1724](https://github.com/BoundaryML/baml/issues/1724), [#4497](https://github.com/BoundaryML/baml/issues/4497) | Closed by v0-only PRs #4202 and #4503. No v1 followup. |

# Retrieval and coverage notes

All GraphQL PR comment/review connections reported complete first pages. REST inline review and issue-comment collections were fetched with pagination. The GitHub reference scan also inspected linked out-of-range PR metadata to distinguish PR links from issues; those PRs are context, not additions to the release inventory. Two regex matches (`#0` and `#55062759600785`) returned 404 and were rejected as non-issue numeric text.

The Ruby PR also links an external Shortcut story. No Shortcut credential was provided or used; its private contents were not verified. The GitHub PR author and reviewer are both included above, so this does not block their contribution acknowledgment. Historical comments and issue state were read at backtest time; they are not a snapshot of what was visible on release day.
