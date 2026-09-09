# Complete release-range inventory

Pinned range: `baml-language-0.18.0..5b398f2b60cfa78258ac9534e8322d056564e85f`. All 74 squash-merged PRs are accounted for. The `baml_language/` path filter finds only 23; the full-range review also covers the installer and developer-facing websites. Nine v0-only PRs are excluded from followups; all 65 remaining PRs were reviewed.

| PR | Language path? | Changelog decision |
| --- | --- | --- |
| [#4625](https://github.com/BoundaryML/baml/pull/4625) feat(cli): embed the BAML agent skill | yes | Include: C04 |
| [#4630](https://github.com/BoundaryML/baml/pull/4630) Interface dispatch bugfixes and unification | yes | Include: C27 |
| [#4633](https://github.com/BoundaryML/baml/pull/4633) refactor(mir): remove dead visualization nodes from MIR | yes | MIR visualization cleanup; no user behavior. |
| [#4623](https://github.com/BoundaryML/baml/pull/4623) Redesign FunctionSpec and streaming projections | yes | Include: C01, C10, C11 |
| [#4643](https://github.com/BoundaryML/baml/pull/4643) fix: concurrent named-client cache lookup race | no | BAML v0 only. |
| [#4642](https://github.com/BoundaryML/baml/pull/4642) engine: bump version to 0.226.2 | no | BAML v0 only. |
| [#4645](https://github.com/BoundaryML/baml/pull/4645) engine: trigger releases manually, instead of by pushing two separate tags | no | BAML v0 only. |
| [#4616](https://github.com/BoundaryML/baml/pull/4616) chore: notify Slack on successful nightly releases | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4644](https://github.com/BoundaryML/baml/pull/4644) chore: make app-product-metrics notifications self-documenting | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4646](https://github.com/BoundaryML/baml/pull/4646) fix(compiler): infer lambdas from callable union arms | yes | Include: C03 |
| [#4632](https://github.com/BoundaryML/baml/pull/4632) fix(installer): bootstrap with compatible Linux wrappers | no | Include: C34 |
| [#4659](https://github.com/BoundaryML/baml/pull/4659) engine: trigger releases from versioned source tags | no | BAML v0 only. |
| [#4663](https://github.com/BoundaryML/baml/pull/4663) engine: fix release CI attestation pagination | no | BAML v0 only. |
| [#4669](https://github.com/BoundaryML/baml/pull/4669) ci: add one-off crates.io 0.226.2 publisher | no | BAML v0 only. |
| [#4670](https://github.com/BoundaryML/baml/pull/4670) engine: fix crates.io release from tag checkouts | no | BAML v0 only. |
| [#4680](https://github.com/BoundaryML/baml/pull/4680) chore: disable ix preview CI, jobs are failing due to compute exhaustion | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4687](https://github.com/BoundaryML/baml/pull/4687) chore(release): move macos builds from gh to blacksmith | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4686](https://github.com/BoundaryML/baml/pull/4686) Fix LSP panic + diagnostic improvements | yes | Include: C28 |
| [#4684](https://github.com/BoundaryML/baml/pull/4684) chore(release): workflow runs are now "canary release: 0.18.0" | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4688](https://github.com/BoundaryML/baml/pull/4688) ci: use shared Infisical credentials for release mirrors | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4692](https://github.com/BoundaryML/baml/pull/4692) fix: make baml_sdk work on cloudflare workers | yes | Include: C29 |
| [#4693](https://github.com/BoundaryML/baml/pull/4693) ci: use shared Infisical token for Swift and Homebrew | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4698](https://github.com/BoundaryML/baml/pull/4698) fix(release): repair nightly toolchain builds | yes | Release-runner repair; no separately demonstrated user runtime effect. |
| [#4685](https://github.com/BoundaryML/baml/pull/4685) ci: move sccache to baml-build3 R2 cache | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4701](https://github.com/BoundaryML/baml/pull/4701) fix(release): fix the x64 toolchain build, again | no | Release-runner repair; no separately demonstrated user runtime effect. |
| [#4714](https://github.com/BoundaryML/baml/pull/4714) Let runtime-compiled code implement and call methods of mounted types | yes | Include: C30 |
| [#4713](https://github.com/BoundaryML/baml/pull/4713) Bind Java publish jobs to the Infisical-synced release environment | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4718](https://github.com/BoundaryML/baml/pull/4718) chore: simplify c# sdk publication | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4611](https://github.com/BoundaryML/baml/pull/4611) [B-1646] Remove union field and method unification | yes | Include: C13 |
| [#4702](https://github.com/BoundaryML/baml/pull/4702) chore: split BAML v1 setup actions from v0 | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4703](https://github.com/BoundaryML/baml/pull/4703) chore: scope the root Rust toolchain to engine | no | Root contributor toolchain configuration; not a v0-only exclusion. |
| [#4704](https://github.com/BoundaryML/baml/pull/4704) chore: bump BAML Language Rust to 1.98.0 | yes | Repository Rust toolchain/CI update; no generated Rust SDK minimum-version change identified. |
| [#4705](https://github.com/BoundaryML/baml/pull/4705) chore: upgrade BAML Language checkout to Node 24 | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4706](https://github.com/BoundaryML/baml/pull/4706) chore: upgrade BAML Language artifact actions to Node 24 | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4707](https://github.com/BoundaryML/baml/pull/4707) chore: upgrade remaining BAML Language actions to Node 24 | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4708](https://github.com/BoundaryML/baml/pull/4708) chore: upgrade BAML v1 on-call actions to Node 24 | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4709](https://github.com/BoundaryML/baml/pull/4709) chore: upgrade BAML v1 product-metrics actions to Node 24 | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4634](https://github.com/BoundaryML/baml/pull/4634) feat(atb2): sandbox for handle_issue: worktree lifecycle, allowlisted env, repro pre-check on canary | no | Internal agent-development pipeline. |
| [#4635](https://github.com/BoundaryML/baml/pull/4635) feat(atb2): the gate: fmt, clippy, changed-crate tests, nextest, insta, the baml corpus; judged by exit code | no | Internal agent-development pipeline. |
| [#4636](https://github.com/BoundaryML/baml/pull/4636) feat(atb2): handle_issue: a design pass, then a fix pass in the sandbox; PR body; outcome; agents run on Fable | no | Internal agent-development pipeline. |
| [#4637](https://github.com/BoundaryML/baml/pull/4637) feat(atb2): handle_issue evals; run_tests.sh picks a pipeline stage; //# headers for the playground graph | no | Internal agent-development pipeline. |
| [#4720](https://github.com/BoundaryML/baml/pull/4720) Invariant-holding type representations | yes | Include: C31 |
| [#4723](https://github.com/BoundaryML/baml/pull/4723) [B-1610] Require matching BAML skill in agent mode | yes | Include: C15 |
| [#4725](https://github.com/BoundaryML/baml/pull/4725) Standard Library conventions and cleanup | yes | Include: C16, C17, C18, C19, C20, C21, C22, C23, C35, C36 |
| [#4639](https://github.com/BoundaryML/baml/pull/4639) feat(app-feedback): the feedback pipeline UI, reading the atb2 store | no | Internal agent-development pipeline. |
| [#4735](https://github.com/BoundaryML/baml/pull/4735) ci: remove C# package availability smoke | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4721](https://github.com/BoundaryML/baml/pull/4721) [B-1682] Scope return type checking to closures | yes | Include: C32 |
| [#4715](https://github.com/BoundaryML/baml/pull/4715) feat(atb2): merge_issue, the PR watcher; package written against canary's BAML | no | Internal agent-development pipeline. |
| [#4738](https://github.com/BoundaryML/baml/pull/4738) ci: use repository secrets for sccache | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4710](https://github.com/BoundaryML/baml/pull/4710) chore(engine): bump engine Rust to 1.98.0 | no | BAML v0 only. |
| [#4711](https://github.com/BoundaryML/baml/pull/4711) chore(engine): upgrade engine actions to Node 24 | no | BAML v0 only. |
| [#4739](https://github.com/BoundaryML/baml/pull/4739) Comparison is total and reflexive | yes | Include: C24, C25 |
| [#4727](https://github.com/BoundaryML/baml/pull/4727) feat(docs): add developer documentation portal | no | Include: C06 |
| [#4730](https://github.com/BoundaryML/baml/pull/4730) feat(docs): publish and render generated references | no | Include: C07 |
| [#4732](https://github.com/BoundaryML/baml/pull/4732) feat(docs): validate and render BAML snippets | no | Include: C06 |
| [#4733](https://github.com/BoundaryML/baml/pull/4733) docs: load authored portal content from canonical sources | no | Include: C06 |
| [#4734](https://github.com/BoundaryML/baml/pull/4734) ci(docs): split portal validation jobs | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4742](https://github.com/BoundaryML/baml/pull/4742) chore(web): remove public changelog | no | Intermediate changelog removal superseded by #4743 and #4783. |
| [#4743](https://github.com/BoundaryML/baml/pull/4743) feat(web): publish releases as blog posts | yes | Include: C08 |
| [#4745](https://github.com/BoundaryML/baml/pull/4745) style(web): apply inline code styling globally | no | Website inline-code styling only; no release-specific product capability. |
| [#4755](https://github.com/BoundaryML/baml/pull/4755) fix(ci): validate nightly release CI run freshness | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4757](https://github.com/BoundaryML/baml/pull/4757) refactor(ci): run release verification after publishing | yes | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4756](https://github.com/BoundaryML/baml/pull/4756) ci: pull language release build images through GCP cache | yes | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4752](https://github.com/BoundaryML/baml/pull/4752) docs(setup): correct README-DEV.md's Rust toolchain instructions | no | Contributor README cleanup; external author is included in followups. |
| [#4729](https://github.com/BoundaryML/baml/pull/4729) feat(atb2): part 5: Supabase store, Slack threads, PostHog intake, run_pipeline, @bammy babysit, a Fly runner deployed from CI | no | Internal agent-development pipeline. |
| [#4759](https://github.com/BoundaryML/baml/pull/4759) Preserve discarded effects and optimize stackified bytecode | yes | Include: C09, C26, C33 |
| [#4762](https://github.com/BoundaryML/baml/pull/4762) Improve generated developer reference discovery and readability | no | Include: C07 |
| [#4764](https://github.com/BoundaryML/baml/pull/4764) Developer docs: render immutable records with live SSR | no | Include: C07 |
| [#4777](https://github.com/BoundaryML/baml/pull/4777) Serve stdlib sources through read-only editor documents | yes | Include: C05 |
| [#4778](https://github.com/BoundaryML/baml/pull/4778) chore(release): improve format of release slack notifications | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4751](https://github.com/BoundaryML/baml/pull/4751) feat(stdlib): add float exp, ln, log2, log10, cbrt, signum, and range constants | yes | Include: C02 |
| [#4779](https://github.com/BoundaryML/baml/pull/4779) chore: document BAML language and wrapper releases | yes | Release/operator documentation, not a language behavior change. |
| [#4780](https://github.com/BoundaryML/baml/pull/4780) chore(release): tag current oncall when a release fails | no | CI, publishing, infrastructure, or internal operational change; no separate user-facing effect. |
| [#4783](https://github.com/BoundaryML/baml/pull/4783) Developer docs: redirect changelog to product release notes | no | Include: C08 |
