# BAML 0.19.0 canary: pinned inventory

LOWER_REF: `baml-language-0.18.0`; LOWER_SHA: `7622555396a99db466afaea09dea2cad259d4033` (excluded).
UPPER_REF: `origin/canary`; UPPER_SHA: `a748b3a694496fa96b043b4bb03b02b529c95584` (included).

Assumed release date: 2026-09-11, America/Los_Angeles (PDT, UTC−07:00). Version 0.19.0 is provisional; the pinned source still stamps 0.18.0. No version bump, publication, tag creation, or user notification is performed by this task.

The fetched [canary manifest](https://pkg.boundaryml.com/manifest/v1/canary.json) identifies 0.18.0 and the lower SHA. Its released_at is 2026-08-29T00:24:04Z; the GitHub release publishedAt is 2026-09-03T22:23:45Z. These are separate timestamps, not the assumed next release date. The latest nightly is 0.18.1-nightly.20260909.a at ffcf6ba7bb6ec82b0eac478a5b8351bff265db47, published on GitHub 2026-09-10T08:20:57Z. It is a different channel and is not used as the previous canary. Repository release instructions are in [RELEASING.md](../baml_language/RELEASING.md). No release.json or baml-language.cfg exists in this checkout; release.toml is authoritative here.

Initial `jj status` reported no jj repository. Worktree: `/Users/sam/baml-worktrees/baml/next-canary-changelog`; branch: `sxlijin/next-canary-changelog`. The child was clean at the pinned upper SHA. The releases-2 parent was not edited.

## Complete unfiltered inventory

All 97 commits are inventoried, including workflows, installers, editors, websites, and v0. Every commit maps to one merged PR. The language-directory-only inventory is supplemental. PR file lists from GitHub can be capped; local merged diffs were used for complete path inventories.

| Commit | PR and title | Changelog decision |
| --- | --- | --- |
| `5f6f54234a3aebb29583c9979c14c32385635a3d` | [#4625](https://github.com/BoundaryML/baml/pull/4625) feat(cli): embed the BAML agent skill | Retain: E01 |
| `ccd81982b812d114a080614799619f96ee7f0180` | [#4630](https://github.com/BoundaryML/baml/pull/4630) Interface dispatch bugfixes and unification | Retain: E02 |
| `48156d3553b76c036a3fc3d8ff441fc757242b3a` | [#4633](https://github.com/BoundaryML/baml/pull/4633) refactor(mir): remove dead visualization nodes from MIR | Exclude: Dead MIR visualization nodes had no producers and emitted no instructions; active playground visualization is separate. |
| `52b927bc99e85362f5d76e43c08bbb3647f099cc` | [#4623](https://github.com/BoundaryML/baml/pull/4623) Redesign FunctionSpec and streaming projections | Retain: E03, E04, E49 |
| `4bfbb6043627568ff3f27428ae63c86bdf183729` | [#4643](https://github.com/BoundaryML/baml/pull/4643) fix: concurrent named-client cache lookup race | Exclude: BAML v0 only; no v1 shipped effect. |
| `3c523b5461e27b014780de7ba13d19ea9a3d97ba` | [#4642](https://github.com/BoundaryML/baml/pull/4642) engine: bump version to 0.226.2 | Exclude: BAML v0 only; no v1 shipped effect. |
| `4c6aea042e0fc3e4fc6df816f93248b1094c40c8` | [#4645](https://github.com/BoundaryML/baml/pull/4645) engine: trigger releases manually, instead of by pushing two separate tags | Exclude: BAML v0 only; no v1 shipped effect. |
| `8268d1a3e8b73ff97e07f0d24cbdb6029f5fe66e` | [#4616](https://github.com/BoundaryML/baml/pull/4616) chore: notify Slack on successful nightly releases | Exclude: Internal release-success Slack notifications only. |
| `4d768289cb0e4db9c866a05fa2a8b05335c76154` | [#4644](https://github.com/BoundaryML/baml/pull/4644) chore: make app-product-metrics notifications self-documenting | Exclude: Internal product-metrics Slack wording only. |
| `02a873758cc011770b5650ca9810e952550b9ef2` | [#4646](https://github.com/BoundaryML/baml/pull/4646) fix(compiler): infer lambdas from callable union arms | Retain: E05 |
| `0655f0908b46c731f8534539c14ab8bfbd60f370` | [#4632](https://github.com/BoundaryML/baml/pull/4632) fix(installer): bootstrap with compatible Linux wrappers | Retain: E06 |
| `b099dbd9d00ae2a9f0cad163ae8da9b119c05e1b` | [#4659](https://github.com/BoundaryML/baml/pull/4659) engine: trigger releases from versioned source tags | Exclude: BAML v0 only; no v1 shipped effect. |
| `d9c0bc720d813821cdd00c90a774bf6f71e4e572` | [#4663](https://github.com/BoundaryML/baml/pull/4663) engine: fix release CI attestation pagination | Exclude: BAML v0 only; no v1 shipped effect. |
| `e4cb45d83ace7877938b91784038dc8f61516307` | [#4669](https://github.com/BoundaryML/baml/pull/4669) ci: add one-off crates.io 0.226.2 publisher | Exclude: BAML v0 only; no v1 shipped effect. |
| `f5a40cc0304e15b7e3ac998cbbbe686f18d14461` | [#4670](https://github.com/BoundaryML/baml/pull/4670) engine: fix crates.io release from tag checkouts | Exclude: BAML v0 only; no v1 shipped effect. |
| `5f8cc28afb899fa01703cd050c83956b0c2f63b9` | [#4680](https://github.com/BoundaryML/baml/pull/4680) chore: disable ix preview CI, jobs are failing due to compute exhaustion | Exclude: Disables an internal preview CI workflow; no installed artifact changes. |
| `194003acd3b15e04cf08fb87a179ca07d44d84d0` | [#4687](https://github.com/BoundaryML/baml/pull/4687) chore(release): move macos builds from gh to blacksmith | Exclude: Changes macOS runner selection only; target artifacts and ABI are unchanged. |
| `cf4c8e516437f1f73d527ab82e245e2f9c4c6670` | [#4686](https://github.com/BoundaryML/baml/pull/4686) Fix LSP panic + diagnostic improvements | Retain: E07 |
| `51a6094b658fbb202945d16c1122a9a7dcd16a50` | [#4684](https://github.com/BoundaryML/baml/pull/4684) chore(release): workflow runs are now "canary release: 0.18.0" | Exclude: Workflow display names and concurrency labels only. |
| `d6d2f13db4e3bf733c5372416dae002a896e9e5f` | [#4688](https://github.com/BoundaryML/baml/pull/4688) ci: use shared Infisical credentials for release mirrors | Exclude: Restores publisher/mirror credentials; no SDK API or artifact-content change. |
| `87cb189696a2b15b9965f3b6ac2292abd4db0ebc` | [#4692](https://github.com/BoundaryML/baml/pull/4692) fix: make baml_sdk work on cloudflare workers | Retain: E08 |
| `8f903030d356b0c19af5b63fec819fde80280fdd` | [#4693](https://github.com/BoundaryML/baml/pull/4693) ci: use shared Infisical token for Swift and Homebrew | Exclude: Publisher token lookup for Swift/Homebrew; no installed binary change. |
| `78c398f1db0ca06efbb4904c959ae8dd2da0a6a1` | [#4698](https://github.com/BoundaryML/baml/pull/4698) fix(release): repair nightly toolchain builds | Exclude: Repairs GNU release builds and runner availability; compatibility floor remains unchanged. Installer behavior is covered by #4632. |
| `b21c1f83cdc9ab0ec3b4cd6b466f3496b9147e59` | [#4685](https://github.com/BoundaryML/baml/pull/4685) ci: move sccache to baml-build3 R2 cache | Exclude: Build cache infrastructure and credentials only. |
| `5a29ca428e3a964d262506ed90d889b3ab4d01a7` | [#4701](https://github.com/BoundaryML/baml/pull/4701) fix(release): fix the x64 toolchain build, again | Exclude: Selects the intended GCC for native build dependencies; retains the glibc floor and linker. No distinct installed behavior beyond #4632. |
| `55409e9da3af6199b0289c03e341a028763b79fd` | [#4714](https://github.com/BoundaryML/baml/pull/4714) Let runtime-compiled code implement and call methods of mounted types | Retain: E09 |
| `95ab5db1cdb277f73abfc10a420a841fb79dc8ab` | [#4713](https://github.com/BoundaryML/baml/pull/4713) Bind Java publish jobs to the Infisical-synced release environment | Exclude: Binds Maven/Gradle publishers to their credential environment; no consumer API or artifact-content change. |
| `f324ec34d44ed781c38c4f99290185c86656ec09` | [#4718](https://github.com/BoundaryML/baml/pull/4718) chore: simplify c# sdk publication | Exclude: Hardcodes existing C# publication settings; same package behavior. |
| `66198e3e9d74743ee6c95dfad8935df0f4b54fc4` | [#4611](https://github.com/BoundaryML/baml/pull/4611) [B-1646] Remove union field and method unification | Retain: E10 |
| `a963a036c3647235daf8a873c1125fc19d23e9f6` | [#4702](https://github.com/BoundaryML/baml/pull/4702) chore: split BAML v1 setup actions from v0 | Exclude: Splits v0/v1 development setup actions; no consumer changes. |
| `fed9f7460297a3d222f97bffdaf6243dd8bf60cf` | [#4703](https://github.com/BoundaryML/baml/pull/4703) chore: scope the root Rust toolchain to engine | Exclude: Scopes developer Rust pins; no installed language requirement changes. |
| `4d6f93a20e4311fd113ab09fbaa795566a037d44` | [#4704](https://github.com/BoundaryML/baml/pull/4704) chore: bump BAML Language Rust to 1.98.0 | Exclude: Build toolchain upgrade; no independently evidenced user behavior or performance claim. |
| `cd406fb25230b8111c0dc9dacd282acb357c8b40` | [#4705](https://github.com/BoundaryML/baml/pull/4705) chore: upgrade BAML Language checkout to Node 24 | Exclude: Checkout action runtime upgrade, not the SDK Node version requirement. |
| `78ee4d3dd836cbcbf40ea0f20549ff4a65f690d4` | [#4706](https://github.com/BoundaryML/baml/pull/4706) chore: upgrade BAML Language artifact actions to Node 24 | Exclude: Artifact action runtime upgrade only. |
| `7a8b98a6b99e43e596b1d277ef0ce28d63e08055` | [#4707](https://github.com/BoundaryML/baml/pull/4707) chore: upgrade remaining BAML Language actions to Node 24 | Exclude: Development/action Node runtime upgrade; no generated client requirement change. |
| `fe2aa78b56348b802e0e7a79d058a738c46db719` | [#4708](https://github.com/BoundaryML/baml/pull/4708) chore: upgrade BAML v1 on-call actions to Node 24 | Exclude: Internal oncall action runtime only. |
| `a70c4edb80ea77d69b38a3705f6aec785f38be08` | [#4709](https://github.com/BoundaryML/baml/pull/4709) chore: upgrade BAML v1 product-metrics actions to Node 24 | Exclude: Internal metrics action runtime only. |
| `e277805fe95a7f087f41e8472a4b98f60878e3c1` | [#4634](https://github.com/BoundaryML/baml/pull/4634) feat(atb2): sandbox for handle_issue: worktree lifecycle, allowlisted env, repro pre-check on canary | Exclude: Internal feedback-agent sandbox implementation. |
| `48a37fc1b3e1a03b807f929b16c56afa2758ffaa` | [#4635](https://github.com/BoundaryML/baml/pull/4635) feat(atb2): the gate: fmt, clippy, changed-crate tests, nextest, insta, the baml corpus; judged by exit code | Exclude: Internal feedback-agent validation gate. |
| `65ea2a442db32e1e9ca06f958f6e8a578f465369` | [#4636](https://github.com/BoundaryML/baml/pull/4636) feat(atb2): handle_issue: a design pass, then a fix pass in the sandbox; PR body; outcome; agents run on Fable | Exclude: Internal feedback-agent fix/design pipeline. |
| `c663dd0451151ed25bacd4d823064bafd7756498` | [#4637](https://github.com/BoundaryML/baml/pull/4637) feat(atb2): handle_issue evals; run_tests.sh picks a pipeline stage; //# headers for the playground graph | Exclude: Internal feedback pipeline tests and graph labels; no shipped playground change. |
| `6d0d6557d9e8e39fac8600c9b818a8b6f9c44f5e` | [#4720](https://github.com/BoundaryML/baml/pull/4720) Invariant-holding type representations | Retain: E11 |
| `b5de6bf0520419d2c608f3c77436cdff53b43216` | [#4723](https://github.com/BoundaryML/baml/pull/4723) [B-1610] Require matching BAML skill in agent mode | Retain: E12 |
| `ceccb6f91047270a82500a2905444c975005fc4b` | [#4725](https://github.com/BoundaryML/baml/pull/4725) Standard Library conventions and cleanup | Retain: E13 |
| `00e3a3b1ebc8aea13fba074a1f6b580066fa6864` | [#4639](https://github.com/BoundaryML/baml/pull/4639) feat(app-feedback): the feedback pipeline UI, reading the atb2 store | Exclude: Internal feedback operations dashboard. |
| `76315e11f892aed0d296fc810ae74b2e7cece87e` | [#4735](https://github.com/BoundaryML/baml/pull/4735) ci: remove C# package availability smoke | Exclude: Removes a flaky C# package availability smoke check; no package contents change. |
| `494e763c9d1a3b28d605ec485a10700ccf2012be` | [#4721](https://github.com/BoundaryML/baml/pull/4721) [B-1682] Scope return type checking to closures | Retain: E14 |
| `aa63c5296436727c699e078dc18dfab822128f52` | [#4715](https://github.com/BoundaryML/baml/pull/4715) feat(atb2): merge_issue, the PR watcher; package written against canary's BAML | Exclude: Internal feedback PR watcher. |
| `eeeb6dd9ed4bc82e477361d4cf02a32fbaaca3ea` | [#4738](https://github.com/BoundaryML/baml/pull/4738) ci: use repository secrets for sccache | Exclude: CI build-cache secret source only. |
| `e7ff97dc1c21da6febee91e6fe6e0b518ef6de50` | [#4710](https://github.com/BoundaryML/baml/pull/4710) chore(engine): bump engine Rust to 1.98.0 | Exclude: BAML v0 only; no v1 shipped effect. |
| `1bdd9b2e74f28c29b81802d6a4426d4ffa3cf101` | [#4711](https://github.com/BoundaryML/baml/pull/4711) chore(engine): upgrade engine actions to Node 24 | Exclude: v0 action upgrade plus shared setup actions; inspected mixed setup diff, no v1 installed effect. |
| `cc49619a697f1b4e3c31284ab90c2beb616cf871` | [#4739](https://github.com/BoundaryML/baml/pull/4739) Comparison is total and reflexive | Retain: E15 |
| `fda82352c0bca2ef65895cf0f5283f3f8bedf6c3` | [#4727](https://github.com/BoundaryML/baml/pull/4727) feat(docs): add developer documentation portal | Retain: E16 |
| `202b2347d8d3d0ead4347c59aa69f64d041dff21` | [#4730](https://github.com/BoundaryML/baml/pull/4730) feat(docs): publish and render generated references | Retain: E17 |
| `abd39f633432268781d427d10a54d5dda956137f` | [#4732](https://github.com/BoundaryML/baml/pull/4732) feat(docs): validate and render BAML snippets | Retain: E18 |
| `f69144ac47f646d0d3aeeb85d6ebe0f7608ea6d3` | [#4733](https://github.com/BoundaryML/baml/pull/4733) docs: load authored portal content from canonical sources | Retain: E19 |
| `8e2f4ea6b1718948e47c9223bde778859e4d2513` | [#4734](https://github.com/BoundaryML/baml/pull/4734) ci(docs): split portal validation jobs | Exclude: Splits documentation validation jobs without changing rendered content. |
| `2f623dcfec9be883bb760e7fd9fc396ea3cd9c4d` | [#4742](https://github.com/BoundaryML/baml/pull/4742) chore(web): remove public changelog | Retain: E20 |
| `085b565a3b51019736bbab66e36df6db4882955a` | [#4743](https://github.com/BoundaryML/baml/pull/4743) feat(web): publish releases as blog posts | Retain: E21 |
| `048bf98acc4c856b4810f44306a7e972c339e70a` | [#4745](https://github.com/BoundaryML/baml/pull/4745) style(web): apply inline code styling globally | Retain: E22 |
| `aefca51d903abe8da7f4c5240f1dde042fec3e58` | [#4755](https://github.com/BoundaryML/baml/pull/4755) fix(ci): validate nightly release CI run freshness | Exclude: Nightly eligibility/CI freshness gate; no separate toolchain behavior. |
| `75e97a6b55208b0424bd667d7ace37f3592eeeeb` | [#4757](https://github.com/BoundaryML/baml/pull/4757) refactor(ci): run release verification after publishing | Exclude: Moves release verification jobs; final publication/promotion logic reviewed at the upper boundary. No installed SDK behavior change. |
| `a6e865549dc52bf574e1d97e31309d690269c6c6` | [#4756](https://github.com/BoundaryML/baml/pull/4756) ci: pull language release build images through GCP cache | Exclude: Container registry mirror and build configuration; same target compatibility. |
| `97d3f5b7bef0acac64066a7b507662abbc63902e` | [#4752](https://github.com/BoundaryML/baml/pull/4752) docs(setup): correct README-DEV.md's Rust toolchain instructions | Exclude: Contributor setup documentation, not language-product documentation. Retained in external followups. |
| `8e9448a79dd2f4c4193acc402866f4bc1aebd3f3` | [#4729](https://github.com/BoundaryML/baml/pull/4729) feat(atb2): part 5: Supabase store, Slack threads, PostHog intake, run_pipeline, @bammy babysit, a Fly runner deployed from CI | Exclude: Internal feedback intake, issue automation, and deployment. |
| `3298f37c81db45bfc6de6d8101fd4cb07e786d1f` | [#4759](https://github.com/BoundaryML/baml/pull/4759) Preserve discarded effects and optimize stackified bytecode | Retain: E23, E24 |
| `dc3ffb266363850caab48a1155bc4e59e7d922aa` | [#4762](https://github.com/BoundaryML/baml/pull/4762) Improve generated developer reference discovery and readability | Retain: E25 |
| `f668991e76493e4f99c12d98358aa868d61f01f4` | [#4764](https://github.com/BoundaryML/baml/pull/4764) Developer docs: render immutable records with live SSR | Retain: E26 |
| `92c9d1e4c0e9135a5590b4a34839b40f0fb0ef14` | [#4777](https://github.com/BoundaryML/baml/pull/4777) Serve stdlib sources through read-only editor documents | Retain: E27 |
| `9184ee38eb6aa88a964f119d5952c5c3ff7461f8` | [#4778](https://github.com/BoundaryML/baml/pull/4778) chore(release): improve format of release slack notifications | Exclude: Internal release Slack formatting. |
| `8a14e352ff9a0276b3de35d4d62a738970287f46` | [#4751](https://github.com/BoundaryML/baml/pull/4751) feat(stdlib): add float exp, ln, log2, log10, cbrt, signum, and range constants | Retain: E28 |
| `1eaf35053245cd2443ecabecdba036aefddb2201` | [#4779](https://github.com/BoundaryML/baml/pull/4779) chore: document BAML language and wrapper releases | Exclude: Maintainer release-process documentation. |
| `b10d1ec78013bed1a8a905499f3509475fed5ccd` | [#4780](https://github.com/BoundaryML/baml/pull/4780) chore(release): tag current oncall when a release fails | Exclude: Internal oncall tagging on release failures. |
| `5b398f2b60cfa78258ac9534e8322d056564e85f` | [#4783](https://github.com/BoundaryML/baml/pull/4783) Developer docs: redirect changelog to product release notes | Retain: E29 |
| `325617677c9e8fb94c4aa9a4d6b1f38e9d3ab083` | [#4496](https://github.com/BoundaryML/baml/pull/4496) Websocket server and API improvements | Retain: E30, E31 |
| `f48e87125e4432e5a13cce2a926e125a77c5e15d` | [#4784](https://github.com/BoundaryML/baml/pull/4784) docs: define changelog preparation workflow | Exclude: This preparation workflow; no shipped language effect. |
| `b9951d7ecb981670d28e594b3482b9a63e22d45c` | [#4786](https://github.com/BoundaryML/baml/pull/4786) Developer docs: require conditional pre-merge CI | Exclude: Documentation required-check configuration only. |
| `1d44ae9d9361c39f837cf78486f06ed1bea746d1` | [#4794](https://github.com/BoundaryML/baml/pull/4794) chore: when building docs, use the shared rust build cache | Exclude: Documentation build cache only. |
| `6bdf52582d5e49f7703bfc0d92fa5292be1b7690` | [#4785](https://github.com/BoundaryML/baml/pull/4785) chore(releases): set up oncall-releases and post reminders every week | Exclude: Internal release oncall scheduling. |
| `75f5bebbb330cde0fface0f034941c13e9996a48` | [#4791](https://github.com/BoundaryML/baml/pull/4791) feat(tools): add Sheep Council Loops email tooling | Exclude: Internal event-email preparation tool. |
| `3c579c95b6112fc7c055909ac4b464af6576f7f1` | [#4798](https://github.com/BoundaryML/baml/pull/4798) chore: migrate Infisical project ID | Exclude: Infisical project configuration only. |
| `d7a9562118ea29099c283f38322e193f6b1c0e54` | [#4788](https://github.com/BoundaryML/baml/pull/4788) fix(website): make changelog posts look nice | Retain: E32 |
| `55ac3d4eeb14ea5f5b5e3947dd6cf9bdf504e6cf` | [#4781](https://github.com/BoundaryML/baml/pull/4781) Fix interface declaration formatting (B-1676) | Retain: E33 |
| `dfc006ff09dad3eebf9fbadb05753ab603c681e6` | [#4803](https://github.com/BoundaryML/baml/pull/4803) ci: trigger developer docs on merge groups | Exclude: Runs documentation CI on merge groups. |
| `d6ba854b937e521b1b4d1c439ed8b4ff635132c2` | [#4736](https://github.com/BoundaryML/baml/pull/4736) ci: disable Windows SDK tests | Exclude: Disables Windows SDK CI jobs, not Windows SDK publication or support. |
| `ffcf6ba7bb6ec82b0eac478a5b8351bff265db47` | [#4800](https://github.com/BoundaryML/baml/pull/4800) Fix contextual throw inference and unify map literal lowering | Retain: E34 |
| `38275af01f5762f409d38d387f4e756d5ac016b3` | [#4804](https://github.com/BoundaryML/baml/pull/4804) Multi-root workspace for IDE + compiler | Retain: E35 |
| `b4cda591b92b3c52ca3cbeeb805c464d76315a49` | [#4799](https://github.com/BoundaryML/baml/pull/4799) fix: use checked lambda signatures during MIR lowering | Retain: E36 |
| `1aac9d2adc601945628c5f1a5b2c17e923cc36e0` | [#4819](https://github.com/BoundaryML/baml/pull/4819) ci: remove ix preview workflows | Exclude: Removes disabled preview workflows; no shipped SDK change. |
| `9adfddcdd3aca9e08bbeb4f6e1f724417d159a6c` | [#4820](https://github.com/BoundaryML/baml/pull/4820) Fix captured interface receiver dispatch (B-1470) | Retain: E37 |
| `b9474410bd41a78f86803d4ddfce8d4e127c3a92` | [#4806](https://github.com/BoundaryML/baml/pull/4806) compiler: one registry for builtin type aliases and their carrier classes | Retain: E38 |
| `cc7298db35d1ff03110eaa44b9921d9d9e5e6ac7` | [#4816](https://github.com/BoundaryML/baml/pull/4816) Replace all_complete with typed all_settled outcomes | Retain: E39, E40 |
| `43779ea1c7730450ab312f1a0b1ec2e84e6afe45` | [#4805](https://github.com/BoundaryML/baml/pull/4805) Add BAML book chapters with annotated examples and checked behavior | Retain: E41 |
| `faec9327eb33b6995c5dab0a3aaad9cfc18e3886` | [#4807](https://github.com/BoundaryML/baml/pull/4807) ai.events: one content-block vocabulary for user turns and tool results | Retain: E42 |
| `8851a21a96ba97eb984fdca20047f7278c23e721` | [#4836](https://github.com/BoundaryML/baml/pull/4836) Prevent stale book styles in production builds | Retain: E43 |
| `6781876655eeb03012de4f36751d0f9600fe4bef` | [#4834](https://github.com/BoundaryML/baml/pull/4834) Make `unreflect` only for local rigid typevars | Retain: E44, E45 |
| `a748b3a694496fa96b043b4bb03b02b529c95584` | [#4808](https://github.com/BoundaryML/baml/pull/4808) engine: call-site argument layouts, reflect witness coherence, throws-never host contracts | Retain: E46, E47, E48 |

## Supplemental baml_language inventory

```text
5f6f54234a3aebb29583c9979c14c32385635a3d feat(cli): embed the BAML agent skill (#4625)
ccd81982b812d114a080614799619f96ee7f0180 Interface dispatch bugfixes and unification (#4630)
48156d3553b76c036a3fc3d8ff441fc757242b3a refactor(mir): remove dead visualization nodes from MIR (#4633)
52b927bc99e85362f5d76e43c08bbb3647f099cc Redesign FunctionSpec and streaming projections (#4623)
02a873758cc011770b5650ca9810e952550b9ef2 fix(compiler): infer lambdas from callable union arms (#4646)
cf4c8e516437f1f73d527ab82e245e2f9c4c6670 Fix LSP panic + diagnostic improvements (#4686)
87cb189696a2b15b9965f3b6ac2292abd4db0ebc fix: make baml_sdk work on cloudflare workers (#4692)
78c398f1db0ca06efbb4904c959ae8dd2da0a6a1 fix(release): repair nightly toolchain builds (#4698)
55409e9da3af6199b0289c03e341a028763b79fd Let runtime-compiled code implement and call methods of mounted types (#4714)
66198e3e9d74743ee6c95dfad8935df0f4b54fc4 [B-1646] Remove union field and method unification (#4611)
4d6f93a20e4311fd113ab09fbaa795566a037d44 chore: bump BAML Language Rust to 1.98.0 (#4704)
6d0d6557d9e8e39fac8600c9b818a8b6f9c44f5e Invariant-holding type representations (#4720)
b5de6bf0520419d2c608f3c77436cdff53b43216 [B-1610] Require matching BAML skill in agent mode (#4723)
ceccb6f91047270a82500a2905444c975005fc4b Standard Library conventions and cleanup (#4725)
494e763c9d1a3b28d605ec485a10700ccf2012be [B-1682] Scope return type checking to closures (#4721)
cc49619a697f1b4e3c31284ab90c2beb616cf871 Comparison is total and reflexive (#4739)
085b565a3b51019736bbab66e36df6db4882955a feat(web): publish releases as blog posts (#4743)
75e97a6b55208b0424bd667d7ace37f3592eeeeb refactor(ci): run release verification after publishing (#4757)
a6e865549dc52bf574e1d97e31309d690269c6c6 ci: pull language release build images through GCP cache (#4756)
3298f37c81db45bfc6de6d8101fd4cb07e786d1f Preserve discarded effects and optimize stackified bytecode (#4759)
92c9d1e4c0e9135a5590b4a34839b40f0fb0ef14 Serve stdlib sources through read-only editor documents (#4777)
8a14e352ff9a0276b3de35d4d62a738970287f46 feat(stdlib): add float exp, ln, log2, log10, cbrt, signum, and range constants (#4751)
1eaf35053245cd2443ecabecdba036aefddb2201 chore: document BAML language and wrapper releases (#4779)
325617677c9e8fb94c4aa9a4d6b1f38e9d3ab083 Websocket server and API improvements (#4496)
55ac3d4eeb14ea5f5b5e3947dd6cf9bdf504e6cf Fix interface declaration formatting (B-1676) (#4781)
ffcf6ba7bb6ec82b0eac478a5b8351bff265db47 Fix contextual throw inference and unify map literal lowering (#4800)
38275af01f5762f409d38d387f4e756d5ac016b3 Multi-root workspace for IDE + compiler (#4804)
b4cda591b92b3c52ca3cbeeb805c464d76315a49 fix: use checked lambda signatures during MIR lowering (#4799)
9adfddcdd3aca9e08bbeb4f6e1f724417d159a6c Fix captured interface receiver dispatch (B-1470) (#4820)
b9474410bd41a78f86803d4ddfce8d4e127c3a92 compiler: one registry for builtin type aliases and their carrier classes (#4806)
cc7298db35d1ff03110eaa44b9921d9d9e5e6ac7 Replace all_complete with typed all_settled outcomes (#4816)
43779ea1c7730450ab312f1a0b1ec2e84e6afe45 Add BAML book chapters with annotated examples and checked behavior (#4805)
faec9327eb33b6995c5dab0a3aaad9cfc18e3886 ai.events: one content-block vocabulary for user turns and tool results (#4807)
6781876655eeb03012de4f36751d0f9600fe4bef Make `unreflect` only for local rigid typevars (#4834)
a748b3a694496fa96b043b4bb03b02b529c95584 engine: call-site argument layouts, reflect witness coherence, throws-never host contracts (#4808)
```

## Review trail

- [Retained PRs](step1b-prs-only-user-visible.md)
- [Per-PR effects](step2a-pr-user-effects.md)
- [Aggregated effects](step2b-pr-user-effects-reaggregated.md)
- [Validation and limitations](changelog-0.19.0-validation.md)
- [External-user research](changelog-0.19.0-followup-research.md)
