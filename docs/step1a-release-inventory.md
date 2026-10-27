# BAML 0.23.0 release audit

Prepared on 2026-10-25. The live canary manifest reports 0.22.2, released 2026-10-13.

- Lower: `baml-language-0.22.2`, `01f5c412c73c213846ea8cb6ead89d303b053672`.
- Upper: `origin/canary`, resolved once to `303930923cd3f2bfc68ebca79dee96f5779175a8`.
- Range includes all 71 commits, across the entire repository.
- Release metadata PR: https://github.com/BoundaryML/baml/pull/5411.
- Exact upper-source compiler: published nightly `0.22.3-nightly.20261024.a` (its tag resolves to the upper SHA).
- Release metadata changes do not change the compiler examples. If more product changes merge before the release, reconcile the range before publishing.

## Full inventory

- `5fe78680b1b057142b2d69466ee4ed1f22b20c4c` feat(v1): add openai-decisions provider (#5200)
- `5a99dfa0979641bec7b9543c26f53fe99ae53a6c` refactor(bex_str): unit-explicit names; add V8/Bun bench ports and workload benches (#5250)
- `5dd71a42cd09a60194cd4c71cb7dac229c96d461` SAP should support generic types (#5252)
- `bc180de2aaac613241bddd36c8fd2d92c3ccd466` fix: fail loudly when MIR lowering reaches an unresolved member (#5243)
- `79a4a7e2f0c701ce766c15c3e9d8db1d68f60b25` Record a streamed LLM function as its own span (#5258)
- `723a98e5d518eb07250bc5da88af885ba6df6171` feat: add image support to Claude Code client (#5225)
- `ded7ade487be1215140303383ae6a7e2569fff19` fix(cffi): teach initialize_runtime to handle trailing slashes (#5223)
- `3caf4c158491a83a16cf4e21f837b1cf30e2965d` fix: improve generics inference to handle a user-reported crash (#5038)
- `03dea8e3c1350a2172e65d1fed1dce63edd9023d` fix(bex_str): deep ropes no longer overflow the stack; O(1) char indexing on ASCII text (#5253)
- `e5b507c9177945b545e684f48562ae46eb1e00db` Eliminate the global `Spelling` table for packages (#5254)
- `16d75a78f85252e22edfc4a11a610ef2b74b6ed9` fix: update rustls, h2, quinn-proto, webbrowser, anyhow to address RUSTSEC (#5271)
- `e65864a1fb74afd97d7f3fa6f60cf5eaef479bd0` fix(generate): exclude tests and testsets from persisted bytecode (#5273)
- `6e6878ae746fc1bc5d0e99c934431e97fa8caf5e` fix: correctly bundle licenses in every baml_sdk (#5275)
- `21993d3759fb5ffe8c4e9827470f7780ea1dee68` fix(cli): fail open if Boundary is unreachable (#5280)
- `00d304c74c77c2a5d192cd48266fe3e530b30bf3` ci: pause CodeTurtle workflow while staging is down (#5290)
- `38f18aa8edece49f898872252574b49c219a2cbc` ci: install the prebuilt wasm-pack instead of compiling it (#5292)
- `47499c22aea9822a48e352ce587bcda265dd1b45` chore: update sheep council tooling (#5295)
- `ee7f68a7f82d275b74a5607ff98cf9c30c0c6cee` perf(bex_vm): derive the runtime package once per function switch (#5296)
- `c84cbeba08a13d8319c8c62f8ba04fab28fc64f4` Rename reflect kind views to reflect.Class, reflect.Enum, ... (#5268)
- `439fce6b4c96da98b1f0ca3623bc78fb70571f9a` Add a static type() method to every type (#5272)
- `2d86152e44c2dd7ad13fd9b6f4399b918d79fc1d` Replace type T = unreflect(e) with let type T = e, with a bounded form (#5278)
- `f544ba10f7987f436bb9624bfd9711c634d1babe` Make reflect lookups throw NotFound and add find_* variants (#5276)
- `b4223ba77297424bf7856186a0a3c7a0b36bbf82` Accept reflect kind views anywhere a type is (#5277)
- `68e17d373f73fe256e5c08e106697e1acd6f9bb2` Add reflect.Class.extends<I>(name, fields) (#5279)
- `424287f23ce884cd9ac676000b8aeab12c0414e8` Add Builder.fields_from to copy a class's fields (#5269)
- `1b7d2dade9c9f024d6b405bedc72feea90b0b646` Make reflect.class.Field.type non-null (#5270)
- `456df424da0262f58b6c1875dcc984583837cecf` fix(release): re-enable AUR publishing (#5298)
- `77e1e998f59739a82d7c8a53063e6ee14cddcf66` feat(release): submit winget manifest updates with komac (#5299)
- `8afeb7e2cfc69cfc6b5c93932e57c819d5d49403` Follow up on the reflection stack reviews (#5297)
- `292a922eeb68cc4675dc8daed614f9b4bd3cdd05` Add owned function spans and pass-through tracing for AI execution (#5282)
- `13e20f5771467e61a1d8b257f4434b9379d80f7d` Model turns as a tree; requested and resolved models; usage on btel spans (#5261)
- `f52b47fe584ca4a745fef219e7f62931a4e229c9` speedtest: workload directories, in-process drivers, KPI report (#5301)
- `481d074234fce9df31ee0ab0da5f29c5d5fe1e4d` Deliver owned spans to BCS (#5310)
- `648ad5ed10f543c129b51789c7d02e29b64f06f7` Highlight let type in the TextMate and tree-sitter grammars (#5316)
- `1626277d2ed5f8197e90b790da9742c0c7a11b02` Accept BAML_CONTEXT in packed executables and baml run (#5319)
- `1720f0d23b2817c4efb4153d8909e9bbd441a96f` fix: give catch, catch_all, catch_all_panics consistent semantics (#5317)
- `a71490d0035025b641b78e02024a883cea05da3a` Name LLM function spans and entry futures after the function (#5335)
- `990035bec18133ed9fcc61b17909eb438a3f8b69` fix: teach bridge functions to runtime-type-check the args they're called with (#5333)
- `5e9826d29212b5ac4814d56c2b75fb0788695d8e` feat(compiler)!: strict string escapes and raw string literals (#5345)
- `0f652d12da43fcb5f063143ae4cae99050faf404` sdkgen_rust: qualify clashing union variant names by namespace (#5320)
- `c521d3c8555c7e460c91d07e803eacef2e7c49d9` chore(deps): bump CA trust stores for eMudhra intermediate CA compromise (#5356)
- `1654df382a96f9e0671e7ab1b5fbf1229ef61a52` feat: global variables (top-level `const`) (#5325)
- `def2a0e3ac75c53499f92bf38b311762b6adc417` feat(reflect): list and look up a package's globals (#5361)
- `cac5dd04590fc506ed1eec38ae905898f05b32e2` docs(lessons): annotate a global with const and a local with let (#5364)
- `6aa168edea177a8225071d607bc69114c6229c78` chore: fix feedback_auth_e2e flakiness (#5363)
- `953c58d5106112662afa1cd4d6d904b97fd42736` fix(parser)!: sane parses for newline-delimited expressions (#5357)
- `58970f8df7f006ecdea835d8cb16ff36b021a306` OpenAI native structured outputs option (#5371)
- `40617e7b7c29ec0bddb7fdb72f869cccaf4da65d` Record class and enum definitions in telemetry (#5169)
- `2ce48b88a97779a555b017e936bfc8f42641e0e4` fix: correctly resolve instance methods on aliases (#5358)
- `2143382aad42d0bd4a70dcc0367890476f2f827b` fix: reject assignment to non-place targets like `[x, y] = [1, 2]` (#5020)
- `907e84218fc5f37809ed70bb1bb2b23dc68e6621` fix(engine): accept class arguments that omit nullable fields (#5291)
- `7d2b41e48f31051d2067cab96742aa674b920f6f` fix(sdk-typescript)!: make nullable class keys optional (#5377)
- `a09a7199a57d491a4ea4239624d4f335af77da84` fix(btel): bound cloud telemetry delivery at shutdown (#5379)
- `f6b65b385e28dcdf75ebc97bb542464db11837ea` fix(btel): send cloud telemetry through the environment's proxy (#5380)
- `6b90032f74e559d699476594308bb670f5730762` fix(sdk): finish in-flight work and telemetry at exit in the Rust and TypeScript bridges (#5293)
- `e373e39ac38b8840a31247fa3e6df1d82fbf8c4d` feat(sdk-rust): accept class arguments by reference (#5372)
- `4da8e1d5964e4b9880a1cfc0d5f9b6927f7a8e37` fix(sys_native): reuse HTTP clients across requests (#5373)
- `83c3abec28f5351f68440e559af8503a555f3341` fix(sdk-rust): box class cycles that pass through type aliases (#5374)
- `51fb016a33276fcf13309f5199049ed87228eb96` fix(sdk-rust): map json to serde_json::Value instead of skipping it (#5287)
- `921d3f15e1ac2bb163b417647be5195accdf0aad` feat(sdk-rust): optional serde impls on generated types (#5375)
- `8e38c25ef89330e1f87e232e7b46febbf5cb05f5` feat(btel): add a publisher setting to keep HTTP bodies out of network spans (#5389)
- `0c11e94620e31b9f3eb79e8f6832e00f5039a6c4` fix(btel): a bad level, empty key or bad URL no longer fails SDK load (#5388)
- `1edc3aac44038d26a0a03adf0e03c5387e2352f4` Fix btel upload stalls (#5390)
- `5d3d89e586f12a4abebcd0f192cbbea702cbbf69` fix(compiler): correctly parse complex union types (#5387)
- `9dbb5f46d3db84132789cae1e98433c37537e2a4` fix: make ?. type checking work, when the scrutinee has nullable type (#5405)
- `285cd380f94fbe3f8c2f4876ee17f0fa7ea4e85f` Drop union members that can't come from JSON instead of rejecting the type (#5398)
- `7950b4de0bbe29d733910706cf25e4baf9acdcfa` One name per conversion in baml.json (#5399)
- `590abae9beffed53bd16b67793dc321bed476972` Read and write JSON without an intermediate serde_json tree (#5402)
- `dc510563660345c7f1b898a4f67b85908b3ad113` Decode JSON unions by best match, and other decode fixes (#5404)
- `608d4bb4ee502708e817c2743a98d6c02ecdea43` Fix error sentinel erasure points (#5401)
- `303930923cd3f2bfc68ebca79dee96f5779175a8` fix(vm): limit each BexThread stack to 10 MiB (#5386)

## Exclusions after inspecting shipped effects

- #5250: Internal BexStr naming and benchmark ports; no standalone shipped behavior.
- #5258: Intermediate streamed wrapper span superseded by #5282; final release records the original function operation instead.
- #5290: Internal review-workflow pause.
- #5292: CI/release build setup installs prebuilt wasm-pack; no changed installed API or artifact content beyond enabling builds.
- #5295: Internal council tooling.
- #5296: Restores recursive VM performance regressed earlier in this range by #5254; no demonstrated release-level speedup over the lower boundary. Retain external PR in thank-you audit.
- #5298: AUR wrapper publishing resumes on the next wrapper release; wrapper remains 0.2.5 in this language-only bump.
- #5299: Winget submission runs on wrapper releases; no wrapper version bump in this release. Do not promise an accepted package update.
- #5301: Internal benchmark harness/KPI reporting, not a shipped language feature. Retain external PR in thank-you audit.
- #5363: Test-only mock keep-alive flake fix.
