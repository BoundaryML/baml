# BAML 0.19.0 changelog validation

This is a documentation-only preparation change. The draft and checklist remain unpublished. The release version/date are provisional; no release artifacts, toolchain version stamps, or external conversations were modified.

## Fixed release boundaries

- Lower, excluded: `baml-language-0.18.0` = `7622555396a99db466afaea09dea2cad259d4033`. This is the latest released **canary** in the package manifest and language tag history.
- Upper, included: fetched `origin/canary` = `a748b3a694496fa96b043b4bb03b02b529c95584`. This SHA stayed fixed throughout research and validation.
- Assumed publication date: September 11, 2026, America/Los_Angeles (PDT, UTC−07:00).
- A newer nightly, `baml-language-0.18.1-nightly.20260909.a` at `ffcf6ba7bb6ec82b0eac478a5b8351bff265db47`, exists. The repository asks for the latest canary that went out, so this canary draft compares with 0.18.0 rather than reporting only the nightly-to-canary tail.
- `release.json` and `baml-language.cfg` are absent. Checked the package manifests, tags, `baml_language/release.toml`, and `baml_language/RELEASING.md`. The upper source version stamp is still 0.18.0; compiler identity below is established by SHA, not its version string.

## Compiler and example validation

Built the pinned upper source on Apple Silicon macOS with Rust 1.98.0 and the locked dependencies:

```sh
cargo +1.98.0 build --locked --manifest-path baml_language/Cargo.toml -p baml_cli --bin baml-cli
```

Used the official lower-bound `aarch64-apple-darwin` release artifact from the canary manifest. Its SHA-256 was verified as `dad86121702f9f6c10ade1b5cd834379848d16910613f96fe398c875556dc67e`. The system/default wrapper was not used to select compiler versions. The initial default Rust 1.91.0 build attempt failed the dependency toolchain requirement; the explicit 1.98.0 build passed without changing any toolchain file.

Extracted each BAML fence into its own temporary project and ran `check` with the appropriate compiler. Used `--agent-skill-check off` for the upper binary to avoid coupling validation to the installed coding-agent skill; the older lower CLI does not support that option. All 26 snippets pass: ten lower-bound migration examples and 16 final-API examples. The two SAP migration snippets were checked during the final audit; the other 24 are unchanged from their successful checks, verified by exact source comparison.

| BAML block | Section | Compiler boundary | Check |
| --- | --- | --- | --- |
| 1 | Float math helpers | upper | Passed |
| 2 | Structured prompts and dynamic values in SDKs | upper | Passed |
| 3 | WebSocket upgrades in HTTP servers | upper | Passed |
| 4 | Function specs, streaming, and generated bindings | lower | Passed |
| 5 | Function specs, streaming, and generated bindings | upper | Passed |
| 6 | Function specs, streaming, and generated bindings | upper | Passed |
| 7 | Shorter standard-library names | lower | Passed |
| 8 | Shorter standard-library names | upper | Passed |
| 9 | Schema-Aligned Parsing error type | lower | Passed |
| 10 | Schema-Aligned Parsing error type | upper | Passed |
| 11 | A dedicated assertion panic type | lower | Passed |
| 12 | A dedicated assertion panic type | upper | Passed |
| 13 | Comparison is total and reflexive | lower | Passed |
| 14 | Comparison is total and reflexive | upper | Passed |
| 15 | Comparison is total and reflexive | upper | Passed |
| 16 | Narrow unrelated unions before member access | lower | Passed |
| 17 | Narrow unrelated unions before member access | upper | Passed |
| 18 | Runtime type bindings are local and rigid | lower | Passed |
| 19 | Runtime type bindings are local and rigid | upper | Passed |
| 20 | Runtime type bindings are local and rigid | upper | Passed |
| 21 | `all_settled` replaces `all_complete` | lower | Passed |
| 22 | `all_settled` replaces `all_complete` | upper | Passed |
| 23 | Agent journals use structured content blocks | lower | Passed |
| 24 | Agent journals use structured content blocks | upper | Passed |
| 25 | WebSocket frames and closure | lower | Passed |
| 26 | WebSocket frames and closure | upper | Passed |

Executed six self-contained example entry points, beyond compile-only validation:

| Example | Boundary | Observed result |
| --- | --- | --- |
| Float helpers (`math_examples`) | Upper | Nine numeric results, including ln(e)=1, log2(8)=3, log10(1000)=3, cbrt(−8)=−2, and signum(−2)=−1 |
| Prompt projection (`preview`) | Upper | Rendered the expected system prompt without an LLM call |
| Assertion recovery | Lower and upper | Both return false with their respective UserPanic/AssertionFailed catch patterns |
| Journal text projection | Lower and upper | Both return Continuefound |

Runtime validation caught a lower-bound assertion detail: assertions already raised a UserPanic at 0.18.0. The migration now changes the dedicated panic type; it does not claim assertions first became panics in this release. Target compilation also corrected nullable stream partials and the panic-context to_string API.

## Enum-match boundary probes

Executed the same four-variant enum match at both boundaries. For Mode? with null, 0.18.0 fails with “VM internal error: type error: expected variant, got any”; the pinned upper returns the wildcard result “other”. For Mode | Other with Other.W, 0.18.0 incorrectly returns the Mode.A result “a”; the pinned upper returns “other”. The #4623 runtime_ty_is_enum_only guard remains at the upper boundary and prevents the unsafe discriminant jump table in both cases. These are executed before/after probes, separate from the regression-suite count below.

## Runtime regression suites

```sh
baml_language/target/debug/baml-cli --agent-skill-check off --project baml_language/crates/baml_tests/baml_src test --include root.interfaces --include root.lambdas --include root.optional_function_parameters --include root.dynamic_impl_registration --include root.floats --include root.explicit_maps --include root.compiler_aliases --include root.scoped_type_bindings --include root.llm_on_event
cargo +1.98.0 test --locked --manifest-path baml_language/Cargo.toml -p bex_engine --test combinators --test host_value_callable
cargo +1.98.0 test --locked --manifest-path baml_language/Cargo.toml -p baml_tests --test future_all_settled
```

- Native BAML selection: **572 passed, 0 failed**. This includes interface dispatch, closures, optional arguments, reflection registration, floats, maps, aliases, scoped type bindings, and local on_event tests.
- Engine combinators: **14 passed, 0 failed**.
- Host callable contracts: **32 passed, 0 failed, 1 pre-existing ignored test**.
- all_settled: **6 passed, 0 failed**. Cases cover ordered failures/successes, empty input, input panic/cancellation context, and collector cancellation.
- Combined regression total: **624 passed, 0 failed, 1 ignored**. These are executions on the native development build, not just type checks.

## Generated SDK and report probes

Generated Python/pydantic, TypeScript/node, and Rust clients with naming_convention = preserve-case at both boundaries. Inspected the actual exports and generated signatures. Python snippets parse successfully. This validates generator output and API names; it does not claim all host-language projects or provider calls were executed.

- Python lower exports Extract__render_prompt; upper exports Extract_spec / Extract_spec_async and Extract_stream / Extract_stream_async. Prompt text/messages methods and final_async are present. Partial models remain under stream_types.
- TypeScript upper exports Extract_spec / Extract_spec_async and retains Extract$stream / Extract$stream_async. Partial Receipt$stream names remain unchanged.
- Source attribution for the Rust fix is #4623: translate_throws filters open-interface arms into Error::Runtime while preserving representable typed arms. The ai.errors.Failure interface remains part of the BAML error contract.
- Rust lower skips the representative Extract LLM function because of ai.errors.Failure. Upper emits direct, spec, and stream bindings. This is the scoped evidence for #4370; it does not resolve every reason a generator can skip a function.
- #4371: generated a Route class with graph.query/graph.diff literal arms and an LLM function returning it. Both boundaries skip the class and function and exit 0. No fixed notification.
- #4506: checked the issue’s exact generic unknown-method reproducer. Both boundaries report E0097 for the unnecessary explicit throws unknown. No new-fix conclusion is drawn from that diagnostic.
- #4750: executed the issue’s signed-zero function. Both boundaries print 0.0,0.0. The draft explicitly leaves the bug open.

## Performance evidence

The #4759 claim is scoped to static O2 bytecode instruction counts, not measured runtime speed. The reviewed analysis compares f793a6484 with pre-PR 048bf98ac: 99,860 to 93,984 displayed instructions, about 5.9% fewer, across the pre-existing snapshot inventory including test/stdlib scaffolding. Build profile for the BAML bytecode is O2; local Rust validation used 1.97.1, while the host Cargo profile for the counting script was not recorded. Counts are unweighted by execution frequency. Inspected the merged optimization at the upper boundary; the implementation survives, but later corpus/call-layout changes prevent presenting this as a whole-release percentage. No earlier superseded or noisy runtime measurements are used.

## Website and review-artifact validation

Loaded the unchanged website get-posts.ts from the child checkout using Node 22.23.2 type stripping and temporary dependencies outside the repository. Both documents have a boolean isPublished: false, are absent from getPosts(), and return null from getPost(slug). The same loader returns 49 other posts as a control. Both bodies compile as MDX with remark-gfm. This is a loader and content-parser check, not a complete Next.js production build or visual rendering test.

Structural checks verify the exact 97 inventory SHAs, 41 retained PRs, 51 effect IDs classified exactly once, every effect included once in aggregation, at most three PRs per aggregate entry, valid pinned-source paths, corresponding PR references in the draft, local review links, balanced fences, and Python example syntax. Markdown prose uses editor wrapping rather than injected hard line breaks. `git diff --check` and the applicable prek hooks passed.

## Limits and publication gates

No live LLM requests, Cloudflare deployments, Linux bootstrap, HTTP/WebSocket network integration, full website build, or full cross-language SDK matrix was run during this documentation task. The linked PRs and pinned implementations support those release claims; compiler checks alone are not presented as proof of provider/runtime fixes.

The independently versioned live wrapper catalog is 0.2.4 and advertises GNU/musl builds for both Linux architectures. Its git tag points to 2fa0d0df4d0898ddc985be5a501c803a8ff35836, before #4632. Published wrapper contents therefore remain a concrete publication gate before both the installer changelog claim and the #4624 notification. The checklist requires a wrapper version bump/publication and four-platform bootstrap verification; if deferred, omit the claim and notification from this release. The original PR records four-platform before/after bootstrap integration runs; they are not a substitute for checking the final published artifacts.

Two legacy v0 Linear entities were unavailable through the supplied key; their GitHub reports and relevant Discord context were readable. This does not leave a v1 target unresearched. All notification drafts are in [0.19.0.todo.md](../typescript2/app-website/blog-releases/0.19.0.todo.md), with provenance in the [followup audit](changelog-0.19.0-followup-research.md). Release version/date, artifact publication, wrapper verification, and sending notifications remain future release-owner actions.
