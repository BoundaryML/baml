# Card Index

All 144 principle cards from `design-principles.md`, listed by batch. Format: **ID** — title · detectability · BEP status weight notes where flagged.


## Functions, interfaces & organization (BEPs 17, 33, 44, 57, 62, 12, 8, 4)

- **P-017-1**: One arrow token everywhere · _static — grep/AST for `): Type {` declaration forms and `=>` outside match arms._
- **P-017-2**: Braces and parens are mandatory on lambdas · _static — parser/grep for lambda forms without braces or parens._
- **P-017-3**: Exactly two function forms; last expression is the value · _hybrid — static for form; light judgment for "gratuitous return" style._
- **P-017-4**: Types-as-values go through `reflect.` · _static — grep for schema-builder calls whose type arguments are strings or bare identifiers rather than `reflect.*`._
- **P-033-1**: Optional, nullable, and defaulted are three orthogonal concepts · _hybrid — the null-then-assign-default pattern is greppable; deciding the default was expressible needs judgment._
- **P-033-2**: Defaulted arguments are named at the call site · _hybrid — positional-vs-named is static; "should have been named" is judgment._
- **P-033-3**: Defaults are call-time expressions — use their power · _semantic — needs LLM judgment that the body prologue is a hand-rolled default._
- **P-033-4**: Use the prescribed replacements for excluded parameter features · _hybrid — name-family and map-typed option params are greppable; "should have been defaults" needs judgment._
- **P-044-1**: Interfaces are for open, extensible dispatch · _hybrid — repeated concrete-class matches are greppable; open-vs-closed intent is judgment._
- **P-044-2**: Conformance is nominal and explicit · _hybrid — mirror-shape-without-implements is mostly static; intent needs judgment._
- **P-044-3**: Default methods eliminate copy-paste conformance · _hybrid — identical bodies across impl blocks are near-static (diff); "should be a default" is judgment._
- **P-044-4**: Fields live on the class; the implements block is a verified contract · _static — field/type comparison against implemented interfaces._
- **P-044-5**: The type is the disambiguator; downcasts are always explicit · _static for `upcast` misuse; hybrid for discriminator-field slop._
- **P-044-6**: `requires`, not inheritance — every conformance is local · _static for duplicated members across related interfaces; semantic for inheritance-mimicry._
- **P-057-1**: Ask "could one type implement this twice?" — associated type vs generic parameter · _hybrid — the redundant-witness-param shape is statically recognizable; the one-impl-vs-many judgment is semantic._
- **P-057-2**: Project associated types instead of threading them · _static — signature analysis._
- **P-057-3**: Interface-typed values must bind their associated types · _static._
- **P-062-1** [DRAFT]: Wrappers preserve the exact signature via `<T extends Function>` + spread · _hybrid._
- **P-062-2** [DRAFT]: Wrappers advertise new failure modes honestly · _semantic — needs reading the wrapper body against its declared channel._
- **P-062-3** [DRAFT]: `class` is the one data type; anonymous records are deliberately absent · _hybrid — map-typed structured data is greppable; "should be a class" is judgment._
- **P-062-4** [DRAFT]: Absence, null, and missing-map-key are three different things · _static — grep `baml.Unset` usage sites and `catch` around `.get`._
- **P-012-1**: LLM functions stay plain functions; low-level access uses `$` companions · _hybrid — manual JSON-parsing next to a declared LLM function is greppable; equivalence to a companion needs judgment._
- **P-012-2**: `$`-qualified names are compiler territory · _static — trivial grep._
- **P-008-1**: Folders are organizational; namespaces are a deliberate opt-in · _hybrid — folder census is static; whether a boundary is warranted is judgment._
- **P-008-2**: Prefix = external, no prefix = local — and no imports, ever · _static — resolution rules are mechanical._
- **P-008-3**: The tree tells the story; collisions are namespaces' job · _hybrid — name-stem clustering is greppable; the "should split" call is judgment._
- **P-004-1** [PENDING]: Visibility ceremony is target-relative — apps shouldn't be littered with `public` · _static — count labeled vs unlabeled declarations against target mode._
- **P-004-2** [PENDING]: Explicit `private` is a promise the compiler keeps · _hybrid — the error-fix pattern shows in history; lockfile churn is static._

## Control flow, patterns, cleanup & concurrency (BEPs 15, 11, 16, 42, 51, 41, 34, 31)

- **P-015-1**: Destructure in the pattern, not the arm body · _hybrid — grep/AST finds `let x: Type =>` arms followed by field extraction and `len(`/index idioms inside arms; LLM judgment confirms a pattern would have expressed it._
- **P-015-2**: Flatten value branching into arms (field literals + guards), not `if`/`else` inside arms · _hybrid — the `if binding == literal` guard is nearly static; nested-conditional-instead-of-arms needs semantic judgment about whether arms could express it._
- **P-015-3**: Minimal `let` — write it only where it disambiguates · _static — these forms are syntactically identifiable (and the formatter would strip them; unformatted-looking pattern code is itself a signal)._
- **P-015-4**: Prefer flat refutable binding (`let ... else`) over nesting · _hybrid — AST can flag `if let` spanning to end-of-function and diverging two-arm matches; judgment needed on whether the success path is "genuinely local."_
- **P-011-1**: Bindings are always explicit (`let`); bare identifiers in patterns are always type lookups · _static — `let _: T` vs `T`, and binder usage, are pure syntax._
- **P-011-2**: Order arms specific-to-general and lean on exhaustiveness; guarded arms need an unguarded fallback · _hybrid — catch-all-on-closed-union is statically detectable; whether the catch-all is *warranted* (genuinely open data, e.g. LLM output) needs judgment._
- **P-016-1**: Use the `catch (e)` binding and per-arm narrowing as implemented — the arm-only-binding style was rejected · _static — unused top-level binding plus universal per-arm rebinds, or redundant type re-checks inside narrowed arms, are AST-visible._
- **P-042-1**: Declare `defer` adjacent to acquisition; never place cleanup after the use site · _hybrid — grep finds `.close()`/`.release()`/`.cleanup()` outside `defer` blocks and duplicated cleanup in catch arms; judgment needed for whether intervening code can throw._
- **P-042-2**: `Cleanup` is a nominal, opt-in safety net — `defer` for determinism, `implements baml.gc.Cleanup` for "if I forget" · _hybrid — `implements Cleanup` presence and defer pairing are greppable; "does this class hold a resource" is semantic._
- **P-042-3**: Don't hand-roll call-once guards or finalizer deregistration — the runtime latch owns idempotency · _static-leaning hybrid — boolean-latch fields named `closed`/`cleaned`/`disposed` guarding cleanup bodies are grep-detectable; occasionally such a flag serves an unrelated purpose._
- **P-042-4**: Let errors chain — don't swallow defer errors or stringify causes · _semantic — needs LLM judgment on whether a swallow/string-concat is defensive slop versus a genuine decision to discard._
- **P-051-1**: Compose with lazy iterator adapters; implement the `Iterator`/`Iterable` interfaces for custom sequences · _hybrid — index-loop and manual-find shapes are AST-detectable; "should this class have been an Iterator" is semantic._
- **P-051-2**: Iterators are single-consumer; one outstanding `next()` at a time · _hybrid — `spawn`-wrapped `next()` on a shared binding is greppable; aliasing analysis of "same iterator" needs judgment._
- **P-051-3**: Respect the fallibility and termination contract of `next()` · _hybrid — `Repeat` + terminal is near-static; in-band error encoding needs semantic review._
- **P-034-1**: No function coloring — concurrency is a call-site decision, never an API split · _static-leaning hybrid — name-pair and return-`Future`-then-immediately-awaited patterns are greppable; intent behind a returned Future needs judgment._
- **P-034-2**: Spawn independent slow work; don't serialize what has no data dependency · _hybrid — `await spawn` adjacency is static; "are these calls independent" is semantic._
- **P-034-3**: Use stdlib combinators and `.then()` normalization, not hand-rolled coordination · _hybrid — `race(` on provider fan-outs and `is_settled` polling loops are greppable; "which combinator was intended" needs judgment._
- **P-034-4**: Never let a wildcard catch swallow `Cancelled` · _static-leaning hybrid — explicit `Cancelled` arms and token-shaped parameters are greppable; whether a Cancelled handler is legitimate needs judgment._
- **P-034-5**: Rate limiting is a shared `TaskGroup` value; cross-cutting spawn concerns go in `with` middleware · _hybrid — retry loops and batch-await shapes are AST-recognizable; distinguishing legitimate custom policies needs judgment._
- **P-034-6**: Fire-and-forget must be `detach`ed and self-error-handling · _static-leaning hybrid — discarded spawn results are AST-detectable; whether the body can throw needs type/semantic info._
- **P-031-1**: Do not write code assuming lazy dispatch — `spawn` starts eagerly *(anti-pattern from superseded BEP)* · _semantic — requires reasoning about shared-state access windows; grep can shortlist writes to spawn-captured bindings._
- _(BEP-041 — Async Iterator Protocol, status: rejected — no cards)_

## Type system & data (BEPs 21, 22, 38, 39, 43, 55, 13, 29, 5)

- **P-021-1**: Use the Temporal taxonomy, not raw numbers or strings, for time · _hybrid — grep for epoch-int fields, ISO-string regexes, and `* 1000`/`* 3600` magic constants near time-named variables; LLM judgment to confirm the value is semantically a time._
- **P-021-2**: Timezone-aware values are absolute-time-backed; never do component-wise wall-clock math · _hybrid — offset-constant arithmetic is greppable; whether component reassembly is a DST hazard needs LLM judgment._
- **P-021-3**: Parse routing by string shape: offset-carrying → `Instant`/`ZonedDateTime`, zoneless → `Plain*` · _static-leaning hybrid — string concatenation of `"Z"`/offset suffixes immediately before `Instant.parse` is greppable; intent confirmation is semantic._
- **P-021-4**: Lossy/fallible conversions are explicit; clock and system-timezone access is swappable IO · _semantic — needs judgment about whether the error handling and `now()` placement are principled._
- **P-022-1**: Use `bigint` where range exceeds `i64`; lean on `int <: bigint` subtyping · _hybrid — hi/lo pair fields and string-number arithmetic are greppable; whether a quantity can overflow `i64` needs domain judgment._
- **P-022-2**: `bigint` serializes as a JSON number, and prefer its infallible/checked methods · _static-leaning hybrid — manual `to_string()` before serialization and hand-rolled numeric loops are greppable._
- **P-038-1**: Use the `json` type for arbitrary/external JSON — never dummy `$parse` functions, hand-rolled escapers, or throwaway wrapper classes · _static — empty-prompt functions, `escape`-named string helpers, and `"\""+ ... +"\""` concatenation are all greppable; single-use wrapper classes need light semantic confirmation._
- **P-038-2**: Conversion into/out of `json` is always explicit · _semantic — requires comparing hand-written conversion bodies against what auto-derivation would produce._
- **P-038-3**: Narrow `json` with pattern matching, not blind assumptions · _hybrid — `stringify` immediately followed by string inspection is greppable; unnarrowed-assumption flows need LLM tracing._
- **P-038-4**: `json` is for external data interchange; typed schemas remain the default — don't sprinkle `json` everywhere · _semantic — a reviewer must judge whether the shape was knowable; the LLM-function-returning-`json` case is a cheap static pre-filter._
- **P-038-5**: `@alias`/`@skip` are LLM-path-only decorators; the JSON interchange path is separate · _hybrid — co-occurrence of decorators and `encode`/`to_json` on the same class is greppable; whether the author expected renaming needs judgment._
- **P-039-1**: `reflect.type_of<T>()` is the single compile-checked bridge from types to `type` values; string lookup is only for dynamic names · _static — literal-string `get_return_type` calls are directly greppable._
- **P-039-2**: `type` values are opaque and compiler-minted; equality is identity — compare and key on them directly, never on stringified names · _static-leaning hybrid — `.to_string()` compared to a string literal near `type` values is greppable._
- **P-039-3**: Write generic functions and let the compiler thread type info — don't hand-monomorphize per-type copies · _hybrid — near-duplicate detection is mechanizable; confirming a generic was expressible needs judgment._
- **P-043-1**: Use the built-in core-type methods; don't re-derive them with manual loops or arithmetic · _hybrid — the loop shapes are greppable patterns; deciding the stdlib method was available and equivalent needs the toolchain's actual method list._
- **P-043-2**: All BAML stdlib and idiomatic user API naming is snake_case · _static — pure regex on identifier casing._
- **P-043-3**: String `length` is byte-based O(1); index operations are fallible on non-ASCII — don't treat length as character count · _semantic — the calls are greppable but the character-vs-byte intent needs LLM judgment._
- **P-055-1** [DRAFT]: Ranges are types unified with pattern matching (`1..3` ≡ `1 | 2 | 3`), and runtime iteration goes through `int.range()` / `float.range()` · _static once shipped (guard-chain and counter-loop patterns are greppable); toolchain-availability check required first._
- **P-029-1** [PENDING]: Immutability is opt-in, deep, and belongs in the type system — not in docs, defensive copies, or parallel class hierarchies · _semantic — parallel-hierarchy and deepcopy-everywhere patterns need LLM recognition; pending status means this grades intent hygiene, not API misuse._
- **P-029-2** [PENDING]: When mutable and readonly variants of an accessor are both needed, prefer explicit Rust-style `*_readonly` method pairs over overloading · _static for name collisions; semantic for union-based mutability discrimination. Low grading weight until implemented._
- **P-005-1** [DRAFT — explicitly unvetted]: The host boundary is snapshot-by-value today; hosts must reassign returned values, and BAML code should not pretend to mutate host state in place · _hybrid — mutate-param-return-void signatures are greppable; whether host code depends on the mutation needs cross-language semantic review. Use only the settled snapshot-copy behavior; treat code implementing the proposed `ref<T>`/`adopt` API as speculative (not gradeable)._

## Standard library surface (BEPs 45, 46, 47, 48, 49, 52, 60, 32, 37, 61)


### Meta-principles (cross-BEP)

- **P-META-1**: Namespaced domain stdlib; top level reserved for universals · _hybrid — grep for local function names matching stdlib surface (`parse_uuid`, `base64_encode`, `glob`, `read_csv`); LLM judges whether they add value or just alias._
- **P-META-2**: Typed domain values, never stringly-typed identifiers · _hybrid — static grep for `string`-typed fields with domain-suggestive names; semantic check on whether the typed alternative applies._
- **P-META-3**: The parse / try_parse / is_valid triad — and when NOT to use it · _static for the spelling mismatches (grep `catch` around `parse`); semantic for whether discarding diagnostics is acceptable in context._
- **P-META-4**: One shared data-stdlib vocabulary: `parse` (untyped) / `decode<T>` (typed) / `stringify` (to text), plus the cardinality split · _static — grep for `query<`/`decode<` immediately indexed or length-checked._
- **P-META-5**: Structured errors — a `kind` enum inside an error class when position matters, bare enum when it doesn't; one error identity per domain · _hybrid — grep catch arms; semantic judgment on whether available structure is used._
- **P-META-6**: Handle classes for compile-once / stream resources; plain data classes otherwise · _static — constructor calls lexically inside `for`/`while` bodies are grep-able; hoistability needs light semantic judgment._
- **P-META-7**: Nothing enters JSON or prompts implicitly; conversions are explicit and canonical · _hybrid — grep prompt literals for loop-built table markup (`|`-rows inside `${for}`); LLM confirms `to_markdown` would fit._
- **P-META-8**: Strict by default, no sniffing; every relaxation is a narrow, named option · _semantic — needs judgment on whether preprocessing duplicates an option._
- **P-META-9**: Small closed option sets are literal string unions; open/domain concepts are enums with readable names · _static — enums with ≤3 variants used only as a single function's parameter type._
- **P-META-10**: Options objects over boolean flags for extensible operations · _static — count trailing bool/optional params in signatures._

### Per-BEP principles

- **P-045-1**: v7 for database/orderable keys, v4 for random identifiers · _hybrid — grep for id-construction via string concat/random; semantic for v4-vs-v7 fit._
- **P-045-2**: UUIDs are identifiers, not cryptographic secrets · _hybrid — grep for `uuid.v4` near token-ish names; semantic confirmation of security use._
- **P-045-3**: Parse at the text boundary; canonicalize formatting through the type · _hybrid — grep for `.replace("-"`, case conversions near uuid names._
- **P-046-1**: Base64 encodes bytes, not text — the UTF-8 step is explicit · _static — grep for base64 alphabet constants or manual 6-bit packing; wrapper detection is grep + light judgment._
- **P-046-2**: Standard Base64 and Base64URL are distinct variants — never mixed, correct padding defaults · _static — grep `replace` of `+`/`/`/`=` near base64 calls; variant-context fit is semantic._
- **P-047-1** [PROPOSED]: Values go in named parameters, never concatenated into SQL text · _static — SQL-shaped strings with concatenation/interpolation of non-literal values._
- **P-047-2** [PROPOSED]: Dynamic identifiers via match-allowlist, not parameters or raw input · _hybrid — static finds identifier concatenation; semantic verifies an allowlist guards it._
- **P-047-3** [PROPOSED]: Callback transactions — commit on return, rollback on throw · _semantic — atomicity need requires judgment; the multiple-execute pattern is grep-able._
- **P-047-4** [PROPOSED]: One long-lived `Database` handle; direct SQL, no homegrown ORM layer as default · _hybrid — connect-in-loop and query_rows-dominance are grep-able; ORM-ness is semantic._
- **P-048-1** [PROPOSED]: Secure by default — system CSPRNG unless an `Rng` is explicitly passed · _hybrid — timestamp/hash-based pseudo-randomness needs semantic recognition; direct `random_int` calls are grep-able._
- **P-048-2** [PROPOSED]: Random constructors live on the types, not in `baml.random` · _static — `Rng.random(` calls followed by manual arithmetic._
- **P-049-1**: One expression language — native `${}` interpolation, never a second template language · _static — grep `{{`, `{%`, `#"` in new code._
- **P-049-2**: Block control flow (`${for}`/`${if}`) so source shape mirrors rendered output · _static — `.map(` + `.join("\n")` inside `${}`; loop-appended string builders feeding prompts._
- **P-049-3**: `"..."` is inert, backticks interpolate — pick the form by intent · _static — escape-density in backtick literals; `+ "\n" +` chains._
- **P-049-4**: Formatting via method chains; loop metadata via the language, not magic · _static — counter variables incremented inside `${for}` bodies._
- **P-049-5**: Plain functions replace `template_string`; tagged templates (not string munging) for domain processing · _static — grep `template_string`, `_.role`, `_.chat`._
- **P-052-1** [PENDING]: Linear-time regex by default; compiled `Regex` objects, safety over feature-count · _hybrid — manual scanners need semantic judgment; `Regex.new` in loops is grep-able (see P-META-6)._
- **P-032-1**: Bun-style read verbs, explicit open modes, one polymorphic `write` · _static — mkdir-then-write sequences; `"r+"` usage without a subsequent `seek`._
- **P-037-1**: `Glob.scan` for pattern-based file finding; `read_dir` is single-level with free type info · _static — recursive functions calling `read_dir`; `exists` immediately before `mkdir`._
- **P-060-1**: Streaming reader is the core; one-shots are sugar — pick by input size, same options either way · _hybrid — `split(",")` on file/LLM content is pure grep; eager-vs-streaming fit is semantic._
- **P-060-2**: Per-record error recovery through the designed channels — never lose the good rows, never lose the diagnostics · _hybrid — grep `on_error: "skip"` and check for any `skipped`/`on_skip` reference in scope; abort-on-first-error shape is semantic._
- **P-060-3**: Handle LLM-emitted CSV with the designed idiom, and schema-typed decode over string sniffing · _hybrid — fence-stripping regexes near `baml.csv` calls are grep-able; the rest is semantic._
- **P-060-4**: `to_string()` is display; format serialization is a namespace function · _static — `.join(",")` feeding file writes or returns typed as CSV._
- **P-061-1** [DRAFT]: Complementary operations share one naming convention — don't mix ecosystems pairwise · _static — casing is pure grep; convention-mixing within a module is grep + light judgment._

## LLM machinery & testing (BEPs 9, 58, 59, 23, 36, 40, 56, 30)

- **P-009-1**: Consume parsed partials by iterating the stream, not via callbacks · _static — grep for `b.stream` usage without a final-response call, or partial-delivery plumbing (queues/callbacks) wrapped around the iterator._
- **P-009-2**: `on_tick` is the one user-facing hook, and only for raw SSE access · _hybrid — statically find `on_tick` handlers; LLM judgment to decide whether the handler is raw-event access (fine) or a shadow partial-parser (slop)._
- **P-009-3**: Retry/fallback/round-robin live in client configuration, shared by streaming and non-streaming · _static — retry loops around `b.Fn(...)` calls are grep/AST-findable; check whether the client already declares (or could declare) the equivalent policy._
- **P-009-4**: Control streaming granularity with `@stream.done` / `@stream.with_state` attributes, not consumer-side filtering · _hybrid — attributes are grep-able in .baml; judging whether consumer-side checks duplicate them needs semantic review._
- **P-009-5**: End streams early with cancellation, not by draining · _static — find `break`/early-return in stream loops without a `stream.close()` / `controller.abort()`._
- **P-058-1**: Mock by naming the callable reference, inside a scope — never by restructuring code for testability · _hybrid — `baml.mock` usage is grep-able; detecting DI-for-testability contortions in production signatures needs LLM judgment._
- **P-058-2**: The replacement lambda is the matcher; conditional stubbing needs no DSL · _semantic — reviewer judges whether mock helpers reintroduce a second stubbing DSL._
- **P-058-3**: Use spy mode and live `super` instead of hand-wrapping real functions · _static for counting-shim patterns and by-name recursion; the spawn/await ordering check is hybrid._
- **P-059-1**: The function is the tool's source of truth — schema and dispatch are derived, never written twice · _hybrid — parallel arg-class + dispatch-glue is AST-visible; judging drift/mirroring quality is semantic._
- **P-059-2**: The model's schema must be a strict subset of the call — host context is never LLM-fillable · _hybrid — field names like `token`/`api_key`/`db` in LLM output types are grep-able red flags; whether a field is truly host context needs judgment._
- **P-023-1**: LLM evaluation is just tests — judges are functions, datasets are functions, evals are testsets; no parallel eval harness · _hybrid — host-side eval loops over BAML functions are findable; deciding they duplicate testset capability is semantic._
- **P-023-2**: Parameterize with `testset` + loops over data; never clone test blocks or hide N cases in one test · _static — repetitive test blocks and for-loop-with-asserts-inside-one-test are AST/grep detectable._
- **P-023-3**: Handle nondeterminism with runners — Quorum for quality distributions, Retry for infrastructure, PassRate for suites · _hybrid — missing `with` on LLM-calling tests and in-body repetition loops are static; Retry-vs-Quorum misuse needs judgment about what's being retried._
- **P-023-4**: Tests are self-contained — setup is a visible function call, never a hidden hook · _hybrid — order-dependence and phantom-setup tests need semantic review; non-`assert.*` checking is grep-able._
- **P-036-1**: Encode quality criteria as tests so they double as optimization objectives · _static — cross-reference LLM functions against tests that call them and inspect assert density._
- **P-036-2**: Prompts must specify output format idiomatically via `{{ ctx.output_format }}` · _static — grep prompts of structured-return functions for `ctx.output_format` and for hand-written JSON-shape prose._
- **P-036-3**: Shared prompt components (type descriptions, template strings) couple functions — factor with attribution in mind · _static for the sharing graph; semantic for whether the coupling is intentional._
- **P-040-1**: Cross-cutting execution policy composes as flat `with` middleware, not inline loops or nested wrappers · _static — repeated retry-loop shapes and spawn-await-immediately are pattern-matchable._
- **P-040-2**: Resource budgets (cost, tokens) are scoped, caller-set policy — not baked into individual functions · _semantic — requires judging where budget policy lives relative to who should own it._
- **P-056-1**: Host code catches `BamlError`/`BamlPanic` wrappers and inspects the typed `.value` payload — never string-matches messages · _static — exception-handler patterns around `baml_sdk` calls are grep/AST checkable._
- **P-056-2**: Panics (including cancellation) are `BaseException`-level and must not be swallowed by broad `except Exception` · _static — broad `except` clauses around `baml_sdk` calls plus cancellation usage are pattern-findable._
- **P-030-1**: Use the generated `baml_sdk` bindings as-is — idiomatic per host language, with paired sync/`_async` variants · _static — imports, wrapper layers, and banned namespace names are grep-able._
- **P-030-2**: Use the companion (modular) API for request-level control instead of reconstructing provider calls · _hybrid — hand-built provider payloads next to a BAML function are findable statically; confirming the companion covers the need is semantic._

## Observability, execution & tooling (BEPs 50, 53, 54, 26, 27, 28, 35)

- **P-050-1** [PROPOSED]: Metrics are language-level, co-located with the function they measure · _Hybrid — grep can find `metric` blocks vs. hand-rolled `*_score`/`eval_*` functions; LLM judgment needed to account for the BEP being unshipped._
- **P-050-2** [PROPOSED]: Trace identity is opt-in at the call site and never pollutes return types · _Hybrid — static grep for `trace_id`-shaped fields on domain classes and string-based `id.set(`/`id.get<` calls; semantic judgment for whether wildcard use is exploratory or entrenched._
- **P-050-3** [PROPOSED]: Express metric timing, composition, and sampling as data dependencies, not control flow or config · _Semantic — an LLM reviewer must trace whether judge calls are duplicated and whether gating is expressed as data vs. control flow._
- **P-050-4** [PROPOSED, accepted in comments]: Batch assertions use `after` blocks, not single-dimensional pass-rate hacks · _Static-leaning hybrid — grep for `PassRate` and for `after` blocks; semantic check for counter-hacks emulating batch assertions._
- **P-053-1** [PROPOSED]: Semantic identity must be invariant to non-semantic edits · _Hybrid — static inspection of what feeds the hash input; semantic review to confirm span/runtime data can't leak in._
- **P-053-2** [PROPOSED]: Distinguish interface vs. implementation and direct vs. effective change signals · _Semantic — requires reading the hashing/versioning design; a static check can flag `DefaultHasher`/`u64` in durable-identity paths._
- **P-053-3** [PROPOSED]: Runtime trace identity is separate from semantic identity, and traces join to code via stable keys · _Hybrid — static grep for trace-event shapes; semantic judgment on whether identities are conflated._
- **P-054-1** [PROPOSED, syntax contested]: Keep compile-time directives and runtime metadata in strictly separate mechanisms · _Static-leaning hybrid — grep for magic-comment patterns (`//baml:`) and hardcoded path-match lists; semantic call on scope._
- **P-026-1** [PENDING]: Two output channels — `to_string` for users, `to_debug` for developers — chosen by the author at the call site · _Hybrid — static: hand-written `to_debug` bodies, `+`-chain prints; semantic: whether the audience choice matches context._
- **P-026-2** [PENDING]: JSON is not a debug format · _Static-leaning hybrid — grep for JSON-encode calls feeding logs/prints; semantic call on whether the destination is a wire consumer or a human._
- **P-026-3** [PENDING]: Interpolation holes are ordinary BAML expressions; formatting is methods, not a mini-DSL · _Static — grep for spec-string-parsing helpers and concat chains._
- **P-026-4** [PENDING]: Prompt strings and interpolated strings are deliberately separate systems — don't build prompts by string assembly · _Hybrid — static: string-typed "prompt" parameters built by concatenation; semantic: whether pipeline rendering was meaningfully bypassed._
- **P-027-1**: The signature is the CLI schema — never write argv-parsing boilerplate for a fixed flag list · _Static — a parameterless `main` whose body is a straight sequence of flag lookups is visible in the AST; the escape-hatch legitimacy check (subcommands present?) is a small semantic step._
- **P-027-2**: Exit codes are explicit (`baml.sys.exit`), and return values go to stdout for composition · _Static — grep for `int`-returning mains with status-code-like literals, terminal `print` of results, and manual JSON dumps at exits._
- **P-027-3**: Configuration and target wiring fail at load time, not at runtime; file-path invocation is hermetic · _Static — inspect `baml.toml` and shell wrappers; hermeticity violations need a light semantic check._
- **P-028-1**: Caught errors stay plain values; the stack trace is an opt-in second binding, never global state · _Static-leaning hybrid — unused `trace` bindings and wrapper-error classes are grep/AST-visible; whether a wrapper class is trace-carrying slop vs. legitimate domain error needs LLM judgment._
- **P-028-2**: Match error types explicitly; panics are not silently swallowed by wildcards · _Static for the mechanics (grep `catch_all`, bare `_ =>` with empty bodies); semantic for whether exhaustive catching is justified at that site._
