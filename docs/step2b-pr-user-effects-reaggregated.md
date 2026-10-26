# BAML 0.23.0 release audit

Prepared on 2026-10-25. The live canary manifest reports 0.22.2, released 2026-10-13.

- Lower: `baml-language-0.22.2`, `01f5c412c73c213846ea8cb6ead89d303b053672`.
- Upper: `origin/canary`, resolved once to `303930923cd3f2bfc68ebca79dee96f5779175a8`.
- Range includes all 71 commits, across the entire repository.
- Release metadata PR: https://github.com/BoundaryML/baml/pull/5411.
- Exact upper-source compiler: published nightly `0.22.3-nightly.20261024.a` (its tag resolves to the upper SHA).
- Release metadata changes do not change the compiler examples. If more product changes merge before the release, reconcile the range before publishing.

Aggregation mirrors the reviewed post. Individual bug-fix bullets each combine at most three PRs; the section-wide reference list below is an index, not one combined entry.

# Global variables

A file-scope `const` initializes once at package startup. Initializers can call functions and depend on other globals. The binding cannot be reassigned, but an array, map, or class held by it can still be mutated and is shared across calls.

Use `const` for globals and `let` for locals. A top-level `let` now produces an error. `Package.globals()` lists globals and client declarations; `get_global<T>` throws on a missing name or wrong type, while `find_global<T>` returns null for a missing name. Initialization follows dependencies; reflection cannot read a global before its initializer runs.

PRs: #5325, #5361, #5364

# One trace for an AI function's full execution

Direct calls, function pointers, function specs, and streams record the original AI function as one operation. A stream's operation includes consumption and final parsing. Its output is the parsed function result. Cloud uploads preserve this operation and its children. Functions with a client expression also keep their own names in traces.

You can create an operation explicitly:

PRs: #5282, #5310, #5335

## OpenAI Decisions

Use `openai-decisions/<model>` or `openai.DecisionsClient.new`. Boolean and float results map to predicates; enums and finite unions map to choices. Classes become named questions for their supported leaves. Float results represent predicate probabilities, not ordered-rubric scores.

The client supports text and images and reads `OPENAI_API_KEY` lazily. ([#5200](https://github.com/BoundaryML/baml/pull/5200))

PRs: #5200

## Images and PDFs through Claude Code

Claude Code prompts travel through standard input as stream JSON. Images and PDFs reach the CLI as content blocks, and long prompts no longer depend on command-line argument limits. Unsupported audio/video content produces an error.

PRs: #5225

## OpenAI native structured outputs

`ChatClient` and `ResponsesClient` accept `structured_output = true`; the default remains false. This sends a JSON schema to OpenAI and decodes the structured response. Schema-aligned parsing remains the default path.

LLM outputs, SAP, and tool arguments now drop union members that cannot come from JSON when another member remains usable. The compiler warns about impossible output members and optional fields that can only be null. Entirely impossible outputs still fail. Schema compatibility changes are covered in the migrations below. `bigint` schemas use numeric strings.

PRs: #5371, #5398

## Build runtime types with less boilerplate

Named concrete types have a static `type()` method. Use `reflect.Type.of<T>()` for type parameters and compound spellings such as unions or arrays. `reflect.Class.extends<I>` copies an interface's fields and implementations. `fields_from` copies a class's fields and their metadata, including `@skip`.

`fields_from` copies fields only; pass implementations to `build` when needed. ([#5272](https://github.com/BoundaryML/baml/pull/5272)), ([#5279](https://github.com/BoundaryML/baml/pull/5279)), ([#5269](https://github.com/BoundaryML/baml/pull/5269))

PRs: #5272, #5279, #5269

## Pass Rust class arguments by reference

Generated required class parameters accept `impl Borrow<Class>`. Existing owned calls work, and callers can reuse a class without cloning it:

Nullable and defaulted parameters retain their existing forms. A borrowed async argument must outlive the future; pass an owned value or `Arc` when spawning work. ([#5372](https://github.com/BoundaryML/baml/pull/5372))

PRs: #5372

## Free-form JSON in Rust SDKs

Rust generation maps BAML `json` to `baml_bridge::Json`, an alias for `serde_json::Value`. Classes and functions using it are emitted instead of being skipped. `json?` becomes `Option<Json>`. A bare `json` union arm overlapping another arm remains unsupported.

PRs: #5287

## Optional Serde support for generated Rust types

Enable the generated crate's `serde` feature:

Supported classes, enums, and unions gain serialization and deserialization. Field names follow BAML declarations. Custom BAML `ToJson` overrides are not applied. Unsupported fields such as bigint, bytes, functions, and handles prevent an implementation; generated documentation identifies the blocker.

PRs: #5375

## Explicit SDK shutdown

Rust registers an exit hook and exposes explicit shutdown. Node exposes `shutdownRuntime` at the package root for exits that skip `beforeExit`.

Shutdown waits for in-flight BAML work and flushes telemetry. Repeated shutdown is safe; subsequent BAML calls fail. Call it before exits that bypass normal hooks, such as Node's `process.exit()`. ([#5293](https://github.com/BoundaryML/baml/pull/5293))

PRs: #5293

## Launch context for packed executables

Packed executables read `BAML_CONTEXT` as inline JSON, `@file`, or `-` for stdin. `baml run` also reads it; `--context` takes precedence. Context and JSON arguments cannot both consume stdin.

PRs: #5319

## Omit HTTP bodies from artifact recordings

Publishers can disable network-span request bodies, response bodies, streamed payloads, and network error text:

Regenerate or repack to bake in the policy. Method, sanitized URL and headers, status, timing, and usage remain available. This setting does not suppress function arguments, parsed results, or function errors that contain provider response bodies. Review those capture policies separately when handling sensitive data. ([#5389](https://github.com/BoundaryML/baml/pull/5389))

PRs: #5389

## Recover runtime types from recordings

Captured classes and enums include their definitions. The SQL helper `baml_code` emits an expression that rebuilds a recorded runtime type; declared types are referenced by name:

Enable input/type-argument capture for the target call to populate `type_args`. Older recordings without definitions cannot reconstruct them. ([#5169](https://github.com/BoundaryML/baml/pull/5169))

PRs: #5169

## Faster JSON conversion

JSON conversion no longer builds an intermediate `serde_json` tree. ([#5402](https://github.com/BoundaryML/baml/pull/5402)) measured its `c9399b85` implementation against the parent implementation plus the same workloads. The measurements use release builds on Linux, pinned to one CPU, and the median of five trials:

These are the PR's workload measurements, not an application-wide latency claim. The measured implementation remains in this release. The later union-decoding fixes measured within noise for most workloads; the union-heavy aggregation case was 1–5% slower than this intermediate revision. Override lookups are cached within a call, not across calls.

PRs: #5402

## Faster ASCII indexing and safer large strings

String ropes cache character lengths and ASCII prefixes. ([#5253](https://github.com/BoundaryML/baml/pull/5253)) compared its `155e9024` implementation with canary `5a99dfa0` in release builds on an Apple M3 Max, using nine alternating rounds for the existing string microbenchmarks. A 20k-character flat ASCII scan fell from 11.9 ms to 138 µs. These measurements exclude BAML VM dispatch. Non-ASCII scans were essentially unchanged; some append-only and short-string workloads regressed. This is a workload-specific improvement.

A separate deep-rope run could build and flatten one million appends where the baseline aborted with a stack overflow. The cached-length implementation remains in the release.

PRs: #5253

## Reflection names, lookups, and type views

Reflection kind views move to `reflect.Class`, `reflect.Enum`, `reflect.Union`, `reflect.Map`, `reflect.Array`, `reflect.Literal`, `reflect.Primitive`, and `reflect.Function`. Constructors become static methods. Lowercase namespaces still contain helpers such as `reflect.class.Builder` and `reflect.class.Field`.

Update generated host calls too, such as Python's `reflect.class_.new` to `reflect.Class.new` and TypeScript's `reflect.class.new` to `reflect.Class.new`.

PRs: #5268, #5276, #5277

## Runtime type bindings use `let type`

Replace body-level `type T = unreflect(value)` with `let type T = value`. Top-level static aliases are unchanged.

The bounded form requires a diverging `else` block. A bound mismatch takes that branch; it does not throw. Build a class before binding it: use `builder.build()`, not `builder.type()`. User-defined `TypeView` implementations work too. `reflect.class.Field.type` is non-null; remove its fallback/null check. Invalid reflection-native arguments now consistently use `reflect.errors.InvalidArgument`.

PRs: #5278, #5297, #5270

## Qualify package-local names through `root`

Partially qualified cross-namespace references and a package's own configured name no longer stand in for `root`.

Before, from another namespace:

PRs: #5254

## Schemas require JSON-compatible types

Schema generation rejects `unknown` instead of treating it as an unrestricted JSON schema. Use a concrete JSON-compatible type. For an arbitrary JSON payload, use `json`:

Review model outputs and tool schemas too. Impossible union alternatives can now be pruned, but an `unknown` value a model must fill remains unsupported. ([#5371](https://github.com/BoundaryML/baml/pull/5371), [#5398](https://github.com/BoundaryML/baml/pull/5398))

PRs: #5371, #5398

## JSON and YAML conversion names

The redundant JSON helpers are removed without a deprecation period.

Instance `v.to_json()` and static `T.from_json(j)` methods stay. `decode` now honors `FromJson` overrides. Missing-field errors identify the containing object in `path` and the missing field in `message`; update consumers of error paths.

PRs: #5399

## JSON chooses the best union match and rejects non-finite text

Union decoding chooses the class consuming the most object keys. Numeric preference is int, bigint, then float; string preference is literal, enum variant, then string. Broad JSON/unknown/map alternatives come last. Declaration order breaks ties. Audit persisted unions whose earlier alternatives intentionally swallowed later ones; add discriminants when shapes are indistinguishable.

Before, this returned `Empty`:

PRs: #5404

## Strings decode escapes strictly

Quoted and backtick strings decode hex, Unicode, and supported control escapes. Unknown escapes are errors. Byte strings accept byte escapes and reject Unicode escapes/non-ASCII source characters. Raw quoted strings preserve their contents; there is no raw backtick form.

Before, a regex backslash could survive as written:

PRs: #5345

## Newlines can end expressions

In blocks and match/catch arms, a newline ends an expression when the next token can start another. A next-line `-`, `(`, or `[` can therefore start a new expression. Inside call parentheses, brackets, and map/object field lists, newlines remain whitespace.

Before, this meant subtraction:

PRs: #5357

## Exhaustive catches must cover their inputs

`catch` remains partial. `catch_all` must cover every ordinary error; `catch_all_panics` must cover ordinary errors and panics. Ordinary wildcard arms do not swallow panics, but explicitly named panic arms can handle them. Chained catches run successively, so a later clause must also cover errors from earlier handlers.

Before, an incomplete exhaustive handler was accepted:

PRs: #5317

## Nullable TypeScript keys are optional

Generated nullable fields and constructor keys now accept omission. Reads therefore include `undefined` under `strictNullChecks`.

Regenerate and update strict consumers and structural assignments. Values decoded from BAML still carry null keys. An inbound object omitting a nullable key must not also contain undeclared keys; remove the extra key or supply the nullable key explicitly. ([#5377](https://github.com/BoundaryML/baml/pull/5377))

PRs: #5377

## Model turns, usage, and query fields

`ai.events.LLMCall` becomes `ModelCall`. Requested and response-reported model identities are separate. Rename `Client.id()` and `FunctionSpec.client_id()` to `requested_model()`.

`ModelTurn.calls` contains only its own calls; traverse `all_calls()` to include retry/fallback child turns. Failures live on the child turn. Replace `ModelTurn.model`/`usage`, `AssistantMessage.client_id`, and custom `TurnMeta` token fields with the new requested/resolved-model and usage structures. `RunResult.usage` is `ModelUsage[]`; token counts are nullable when unreported. The journal's separate `Usage` event is removed; consume usage from `ModelCall` events.

PRs: #5261

## Fix 1

- Generic LLM return types retain their realized type arguments during schema-aligned parsing. ([#5252](https://github.com/BoundaryML/baml/pull/5252))

## Fix 2

- Generic receiver inference handles previously crashing arithmetic/method chains. Unresolved members and inference failures produce diagnostics instead of silently reaching emission; legitimate `unknown` values remain usable. Replace map dot access with indexing if diagnosed. ([#5038](https://github.com/BoundaryML/baml/pull/5038)), ([#5243](https://github.com/BoundaryML/baml/pull/5243)), ([#5401](https://github.com/BoundaryML/baml/pull/5401))

## Fix 3

- Optional chaining handles nested nullable unions, and complex function types keep their callable structure inside unions. ([#5405](https://github.com/BoundaryML/baml/pull/5405)), ([#5387](https://github.com/BoundaryML/baml/pull/5387))

## Fix 4

- Instance methods resolve through aliases. Invalid assignment targets such as `[x, y] = [1, 2]` are rejected; assign to variables or indexed/field locations instead. ([#5358](https://github.com/BoundaryML/baml/pull/5358)), ([#5020](https://github.com/BoundaryML/baml/pull/5020))

## Fix 5

- Host-call arguments are checked recursively against declared types. Invalid scalar, list, map, or class arguments report type errors instead of reaching unsafe VM operations. This does not claim to close every opaque-handle or host-return validation gap. ([#5333](https://github.com/BoundaryML/baml/pull/5333))

## Fix 6

- Omitted nullable host class fields become null. Missing required fields still fail, and undeclared keys beside an omitted nullable field are reported. ([#5291](https://github.com/BoundaryML/baml/pull/5291))

## Fix 7

- Swift generated wrappers no longer shadow an argument named `args`. Regenerate affected SDKs. ([#5261](https://github.com/BoundaryML/baml/pull/5261))

## Fix 8

- Rust union variants gain namespace qualifiers when short names collide, restoring affected generated functions. Match the regenerated variant names. Rust class cycles through aliases are boxed; use `Box::new` when constructing newly boxed fields. ([#5320](https://github.com/BoundaryML/baml/pull/5320)), ([#5374](https://github.com/BoundaryML/baml/pull/5374))

## Fix 9

- Generated SDK bytecode excludes tests/testsets and avoids embedding their machine-specific paths. Regenerate to receive the deterministic payload. SDK distributions include the Apache license. ([#5273](https://github.com/BoundaryML/baml/pull/5273)), ([#5275](https://github.com/BoundaryML/baml/pull/5275))

## Fix 10

- Runtime initialization accepts directory names ending in forward slashes. ([#5223](https://github.com/BoundaryML/baml/pull/5223))

## Fix 11

- Native HTTP operations reuse clients and connections for matching timeout/proxy/TLS settings instead of building a client per request. ([#5373](https://github.com/BoundaryML/baml/pull/5373))

## Fix 12

- Boundary telemetry failures warn and continue by default. Explicit abort policies still apply. Cloud delivery honors proxy environment variables, and final delivery has a five-second budget; remaining data can be dropped when the budget expires. ([#5280](https://github.com/BoundaryML/baml/pull/5280)), ([#5380](https://github.com/BoundaryML/baml/pull/5380)), ([#5379](https://github.com/BoundaryML/baml/pull/5379))

## Fix 13

- Generated SDKs and packed artifacts tolerate an empty or invalid telemetry level by recording nothing. Empty destination settings mean unset; an invalid cloud URL falls back to local recording. CLI and source-runtime validation retain their actionable errors, and other credential failures can still reject loading. ([#5388](https://github.com/BoundaryML/baml/pull/5388))

## Fix 14

- Telemetry preparation/uploads proceed concurrently, with memory budgets released as captures advance. Upload acknowledgments do not skip unstored recording files. ([#5390](https://github.com/BoundaryML/baml/pull/5390))

## Fix 15

- JSON cycles produce serialization errors; deeply nested values no longer rely on the intermediate recursive conversion tree. ([#5402](https://github.com/BoundaryML/baml/pull/5402))

## Fix 16

- VM stacks have a 10 MiB budget per BAML thread, counting frame and value slots, instead of a fixed 256-frame limit. Overflow raises catchable `baml.panics.StackOverflow`; use a named panic arm or `catch_all_panics` to recover. ([#5386](https://github.com/BoundaryML/baml/pull/5386))

## Fix 17

- Editor grammars highlight runtime `let type` bindings. ([#5316](https://github.com/BoundaryML/baml/pull/5316))

## Fix 18

- Shipped Rust networking/TLS dependencies and trust stores are updated. Upgrade toolchains and bridge packages to pick up the changes. ([#5271](https://github.com/BoundaryML/baml/pull/5271)), ([#5356](https://github.com/BoundaryML/baml/pull/5356))

## Review and validation

- Final reflection API uses baml.Concrete.type(), not an intermediate Reflectable interface.
- Bounded let type requires diverging else; pending builder values are rejected. Do not copy intermediate PR #5278 semantics.
- Stream wrapper span #5258 is superseded; the final operation is the original LLM function.
- Rust Serde remains first-fit while final BAML JSON decoding is best-match. Do not claim full interchange equivalence for ambiguous unions.
- http_bodies=false does not veto function captures or error bodies. Issues #5249/#5309 are only partially addressed.
- Cloud delivery shutdown is bounded, not asynchronous stdout closure. Issue #5392 gets this narrower claim.
- #5296 restores an in-range regression, so no release-level performance feature is inferred.
- JSON measurements: PR #5402 head c9399b8534e24daa9cef2dce827755a188715f12, compared with its parent implementation plus identical workloads; Linux, release, one CPU, median 5 trials. Merged implementation 590abae9beffed53bd16b67793dc321bed476972 is retained, with #5404 semantic fixes. Reported later aggregation regression is qualified.
- String measurements: PR #5253 head 155e90242dd8c4bcaefb223538c80d755fff9c58 against canary 5a99dfa0979641bec7b9543c26f53fe99ae53a6c; Apple M3 Max release, alternating medians, BAML VM dispatch excluded. The changed BexStr implementation has no subsequent diff through upper. Non-ASCII and regression limits are retained.
- All 29 BAML code blocks compile with the appropriate compiler: 18 final examples at the exact upper-source nightly and 11 before examples at 0.22.2. Thirteen offline assertions pass on final APIs; both schema migration examples also execute successfully on their matching release. Provider examples are compile-only, with no live-provider claims. The recorded-type SQL example executes successfully against a new local recording. Frontmatter and MDX compilation pass, and the actual website getPosts/getPost loader lists the release while excluding the hidden checklist. Benchmark-head vs merged source comparisons show no change in the measured BexStr/JSON implementation at merge; later JSON semantic changes are explicitly qualified.
- All merged PR bodies and 213 issue-comment/review/inline-review responses were audited for identities and references; substantive report discussions and shipped effects were reviewed. Internal CI/bot-only links are not treated as resolved user reports.
- 29 directly referenced Linear issues and their comments/attachments were inspected. They identify internal authors or mirror the GitHub issues; no Discord report links were found in inspected sources. No Discord notifications are invented.
- References #3084 and #5283 are unavailable; #3084 is an unrelated bot-linked reference, and #5283 is the internal reflection review referenced by #5297, whose shipped effects were checked directly. Neither is evidence for an external notification.
- Issue #5300 (redirect policy) is not fixed by connection pooling; do not notify it as resolved. V0-only bot references #3072/#3082 are not announced as v1 fixes.
