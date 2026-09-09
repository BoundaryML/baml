# Interface implementation plan

Current requested work: implementation has resumed under the active goal. Preserve the internal review decisions and the local/published BEP and website hold. Record implementation evidence separately from proposed signatures.

Status: implementation is ongoing on `codex/interfaces-across-bridges`. Checkpoint 3 (shared reference/invocation/ownership protocol) and the Python vertical slice are incomplete; checkpoint boxes stay unchecked until their full acceptance conditions pass.

Current implementation: checked native invocation and exact live type arguments; transactional encoding and owned result receipts; Python/Node issuing-runtime ownership and codec capture for calls, callbacks and background errors. Python generates retained interface and concrete callers, directional inputs and callback types (F29–F47). Node now also generates interface and concrete callers, invariant input evidence, directional containers/aliases/callbacks, and class-aware decoding (F48–F50). Both use compiler-owned class projection and resolved method contracts. Runtime-owned classes keep their dedicated codecs. The detailed progress entries distinguish verified behavior from remaining API work.

Latest completed verification (F60): private native registration/projection with actual callback completion passes **6/6 Python** and **5/5 Node**, including default methods, retained associated-type evidence, owner-language state changes, invalid return rejection and discarded/rejected ownership. The final shared administrative tests pass **5/5**; Python's existing replaced-runtime cases pass **2/2**; Node's new test file type-checks. C++ protocol checks pass **2/2**. F59's shared type/engine/ownership selection also completed **66/66**. These private endpoints do not complete generated host binders or the remaining bridge ports. Artifact format remains 7. Last executed SDK parity remains F53 (735 IDs, 4549 required gaps, no weakened requirements). BEP/site remain on hold.

Next work: complete free-function generic inference/evidence, generic copied-record constructor metadata, directional generic inputs and static bounds; complete ordinary/advanced host implementations, callable lifecycle APIs, executor/admission/cancellation handling, scopes/cycles and the remaining browser/native SDK ports. Preserve compiler-proven interface requirements rather than inferring membership from host method shapes. Complete client overrides and Agent.run execution/streaming, media/PIL, the canonical alias cutover, mandatory ABI/schema agreement, and shared test execution in all supported environments. Keep one PR.

Callback arguments now retain the issuing runtime and registered SDK codecs in Python and Node (F46–F49); received interfaces are invocable. The unresolved work is full scheduling, cancellation, host conformance and cross-language coverage, not a fallback to the current global singleton. Python's narrow checked declaring-view specialization is verified, and F51 adds the corresponding generated Node path, with typed selection and emitted-declaration checks.

Implement [BEP-073](https://beps.boundaryml.com/beps/73) in one coordinated PR. The semantic contract is [the design](MEDIA_INTERFACES_AND_BRIDGES_DESIGN.md); user examples are in [the guide](INTERFACES_USER_GUIDE.md). Read [the adjustment log](INTERFACES_DESIGN_ADJUSTMENTS.md) for changes and unexpected findings from this review.

## Next continuation — host implementation registration

F53–F57 provide declaration constraints, private coherence checking, owned method bodies and checked VM adapter registration. The remaining implementation sequence is:

1. Keep the BAML declaration as the only runtime method contract. The VM now checks supplied method names, required/default selection, inherited fields, exact bindings and coherence before publication. Generated native Host contracts and binders still need compatibility/calling-convention checks; do not add a redundant native-signature algebra to the ABI.
2. F58 supplies owning engine type/instance handles and exact retained-type projection. F59/F60 provide shared encoded operations and verified private Python/Node native delivery. Complete the C receipt ABI and remaining bridge ports with the same transactional ownership and issuing-runtime checks. VM-local registration results cannot cross an await or released heap permit without a root; translate bridge evidence into the engine descriptor under checked ownership.
3. Carry the full generic frame into advanced handlers, including unused method type arguments; realized value contracts cannot recover those choices. Add receiverless host methods through the registered type owner. Both are explicit current limitations.
4. Implement native binding/encoding against that shared path. Verify ordinary methods, explicitly registered advanced handlers, associated-only interfaces, defaults, context injection and local-call behavior with generated positive/negative examples. Binding never trial-runs a body and never infers associated Error from a native exception channel.
5. Add matching canonical SDK cases, then port and execute each supported bridge. For every backend distinguish generated, native type-checked and executed evidence. Existing VM/engine passes do not substitute for those cases.

Implementation has resumed; F60 records private Python/Node registration execution below. Generated binders and remaining bridges retain their execution gates.

Artifact format 7 requires matching native runtimes and regenerated bytecode. No backwards-compatibility layer or public API claim is needed for earlier development artifacts.

## Planning progress

- [x] Read the BEP changes and the relevant type-system, bridge, generator, and test-harness sources.
- [x] Identify both CI coverage checks and their limits.
- [x] Reconcile local host calls, ordinary generic adapters, explicit advanced handlers, and the deferred field boundary.
- [x] Specify ordered work, shared test cases, and native execution gates.
- [x] Include the user's CI fallback for local runs taking roughly four minutes or more.
- [x] Review document consistency and record unresolved API choices separately.

## How to use this plan

Publication hold: leave both the local BEP draft and published BEP unchanged until the user asks to revise them. Record review decisions here and in the internal design/guide. Implementation progress below is existing work, not a claim that the proposed SDK APIs are available.

The earlier feedback review expanded the codegen review with a returned Source factory and caller signatures in four languages, separate factory/interface errors, a complete advanced-handler call sequence, and an empty-array factory. F21/F22 record subsequent implementation separately. This review continues the internal documentation work with required-interface binding preservation and explicit non-class conversion rules. These are proposed contracts, not generated-code validation.

Work through the checkpoints in order, keeping one PR. For each checkpoint, record changed files, the exact checks run and their result, and any remaining issue in the progress log. Check a box only after its acceptance conditions are met. Changes to the design or language support go in the adjustment log with their evidence, then into the design and examples. Do not preserve an obsolete ABI or add legacy adapters merely to keep old snapshots unchanged.

Start sequentially. There is no delegated work now. Once the shared descriptor, reference protocol, and Python vertical slice are stable, an independent backend port or a focused ownership review could justify a subagent. Do not split the initial runtime ownership design, generic adapter decisions, or shared fixture naming between concurrent agents. A delegated backend must run its actual native tests and report file scope and exact results.

## Review feedback to close before revising the BEP

Latest feedback continuation is **internal documentation only**. The [three type choices](INTERFACES_CODEGEN_REVIEW.md#three-type-choices-users-should-not-confuse) distinguish fixed associated bindings, per-invocation method generics, and evidence missing from a required-interface view. Associated bindings alone do not require an advanced handler. The [review packets](INTERFACES_CODEGEN_REVIEW.md#review-packets-for-the-next-bep-revision) specify the exact signatures, invocation, output and negative check needed for each user task. That documentation pass did not verify or advance the existing implementation. Subsequent implementation and verification through F50 are recorded separately below.

- [ ] Assemble the Python/TS packets for existing clients, Agent calls, returned interfaces and ordinary host implementations from actual generated files.
- [ ] Add Go/Rust equivalents with exact success/error types and distinguish callable support from host-implementation support for each member.
- [ ] Validate advanced generic handlers separately from ordinary associated-type implementations; state the fallback or precise diagnostic for each backend limitation.
- [ ] Validate non-class receiver execution and native collection rejection/conversion, including empty and nullable collections.
- [ ] Record generation, native positive/negative checking and bridge execution separately for every packet. Preserve canonical shared SDK case names and report missing environments; the parity linter alone is not execution evidence.

Internal review (A12, source audit updated by F30): [the codegen walkthrough](INTERFACES_CODEGEN_REVIEW.md#6-how-the-generator-gets-from-baml-to-those-signatures) now connects checked metadata, substitution, directional native types and executable decoding. Source inspection confirms the graph exists but ordinary symbol conversion and Python emission/decoding still lose parts of the contract. A12 changed documents only; F30 subsequently implemented and exercised the generated Python Ref path. The prior implementation and verification entries remain separate.

| User question | Internal decision / remaining proof |
|---|---|
| What does an associated type generate? | Design §9.2.1 pairs caller, input and implementation roles. Compile complete Python/TS examples, then Go/Rust, with exact Output/Error pins, defaults and intentionally incompatible assignments. An annotation or cast is not runtime type evidence. |
| What are the exact argument and return types? | For every example, include imports, the generated signature, construction and invocation, the awaited success type and failure channel. Check the snippets against generated files; signature sketches alone do not close this item. |
| What is `ctx`, and who supplies `call`? | Distinguish optional caller options from SDK-supplied `HostCallContext` and advanced `GenericCall`. Introduce each before use. Verify ordinary local methods and separately registered handlers cannot be confused by the type checker or binder. |
| What is generated for `ResponsesClient`? | A live facade for the BAML implementation, with its real factory and implemented methods. Compile direct client overrides and `Agent.run(spec)` without `.as_client()` or rebinding; show their actual result fields. |
| Can arrays or strings implement an interface? | Preserve the compiler's out-of-body rules. Test copied concrete collections separately from retained interface views, including empty arrays, nullable elements versus the Done sentinel, wrong pins and undeclared native adapters. Design §9.14 specifies when each failure is detected. |

These decisions are documented; their implementation and native validation are still pending. Keep backend limitations explicit rather than replacing a type with `any`/`unknown` or omitting the member.

The A11 internal feedback pass: the codegen review now separates compilation, generation, native checking, binding, call admission and host completion. Binding must never trial-run user code. Completion failures do not roll back host side effects. That pass changed documentation only; implementation has since resumed under the active goal (F25). The BEP/site remain on hold.

Review [Interface codegen: decisions to review](INTERFACES_CODEGEN_REVIEW.md) for the paired Source Output/Error example and concrete failure stages. In particular, keep an associated Error parameter even when the native method only exposes an untyped error channel; do not infer a runtime-created BAML type from the shared native DynamicValue wrapper. Ordinary Python host context is optional for local callers and supplied by the bridge. Context-name collisions and dynamic binding signatures remain compile-check gates.

For the eventual external revision, use this reading order: use a returned BAML implementation; pass a generated client as an override; inspect the result of `Agent.run`; declare a native implementation with ordinary methods; explain associated types with paired caller/implementer signatures; then introduce advanced handlers and `call: GenericCall`. Every stage must introduce its types before using them. Collect complete, compile-checked Python/TS examples first and Go/Rust equivalents next; this prioritizes review, not a reduction in bridge coverage.

## Ordered implementation

### 0. Freeze the remaining host API choices with small probes

- [ ] Classify each method projection as an ordinary typed adapter, ordinary method over checked dynamic arguments, or explicit advanced handler. Keep the internal realized call frame independent of that choice.
- [ ] Probe native generic identity over a primitive, generated record, runtime-created record, and live reference; then a generic bound operation and `FunctionSpec<Out> -> RunResult<Out>`. Include an intentionally wrong result and a native implementation that only supports finitely many types. Do not claim that either implements a universal contract.
- [ ] Define separate generated handler declarations/builders and `bind_handlers`; keep `echo(value)` distinct from a handler receiving `GenericCall`. Confirm that handler configuration cannot accidentally satisfy ordinary `EchoHost` registration.
- [ ] Freeze Python optional context injection, parameter-name collision handling, and Pydantic construction behavior. Ordinary calls on the original Python implementation remain local; a checked SDK ref and BAML defaults still use the SDK.
- [ ] Compile-check paired generated caller/input/host signatures for associated types in Python, TS, Go and Rust. Include associated Error, exact defaults, multiple pins, runtime-created types, and the native result/exception shapes. Record the Rust minimum-toolchain decision before emitting `!`; current 1.93 probes reject `Error<!>`.
- [ ] Check error-only associated parameters and invariant ref/input identities even when native exceptions are untyped. Verify per-instance exact tokens for runtime-created types sharing one native dynamic wrapper; static Rust BamlValue metadata alone cannot identify these bindings.
- [ ] Compile host context-name collisions and ordinary local calls without context. Validate that handler configuration and ordinary implementation types remain distinct. Review `Iterator<Item=Done>` union overlap without adding a wire discriminant absent from the BAML contract.
- [ ] Pair the Source factory's `throws never` with the returned ref's `Error=ReadError` in generated callers. Verify these remain separate contracts in Python/TS/Go/Rust, including errors from later `read()` calls.
- [ ] Compile required-interface projection with nondefault associated errors: Iterator to Iterable preserves Item/Error, including nested return types and different `iter`/`next` effects. Reject a projection that reapplies Error=never; validate the complete host implementation bundle before publishing its registration.
- [ ] Exercise incoming context on a standalone host callable and an interface method. Both must carry invocation cancellation/tracked work; keep context-free callback convenience through declared adapter metadata, not speculative calls or argument-count guessing.
- [ ] Generate concrete-Self and receiverless witness APIs alongside existential refs. Reject an existential `combine(other: Self)` overload at generation/type checking; check dynamically supplied concrete evidence before entering the body. Do not treat an ordinary-adapter limitation as absence of the BAML interface.
- [ ] Record native-adapter eligibility per language, with a concrete reason and a supported explicit handler fallback where needed. All generic *callers* must remain usable even if implementing that method needs the advanced path.

Acceptance: a short, evidence-backed generated API table for every active backend, including context/handler signatures. These probes establish language feasibility; they do not replace real bridge tests. Add only probes needed to settle these choices, rather than repeating existing media or state probes.

### 1. Prepare the shared SDK test harness

- [ ] Add the valid shared fixture `sdk_tests/fixtures/interfaces/baml_src/`. Keep negative BAML compile cases in compiler tests so fixture loading does not fail every generator.
- [ ] Add real native tests under `sdk_tests/crates/<generator>/interfaces/customizable/`, using the canonical case names below. Write TypeScript once in `typescript`; `typescript_web` copies it for Chromium and workerd.
- [ ] Register new Rust files in `harness_setup/src/rust.rs::TEST_MODS`. They must be `Gate::Now` when their checkpoint is marked done. Add `interfaces` to Java's compile/runtime green-fixture lists when runnable; inspect equivalent Swift/C++ gates.
- [ ] Add C# support for generating from this shared BAML fixture. Keep its existing native runner model if useful: checked-in Rust wrappers can dispatch individual cases in a C# consumer, but each case must execute its named native assertion.
- [ ] Extend C# parity discovery to those per-fixture wrappers and give them the `interfaces` category. Preserve legacy `src/lib.rs` wrappers as `integration`. Test normalization, duplicates, annotation behavior, and baseline compatibility in the parity tool; do not relabel all old tests or weaken the existing baseline.
- [ ] Replace the Rust SDK harness's nested `cargo test` invocations with nextest before using that execution path. Keep Cargo invocations rooted at `baml_language/`, pin generated manifests, retain the shared target directory and native library environment, and verify nested runs do not recursively select SDK setup jobs. Use nextest's listing/build step for prewarming, not `cargo test --no-run`. Update Unix/Windows scripts and runner labels together.

Acceptance: harness self-tests and a simple existing scalar-call smoke case prove discovery, native execution and case naming without requiring the new interface ABI. Interface behavior is then enabled and proven in checkpoints 4–6. Missing backend support may be visible during development; it is not a reason to add no-op tests or count a disabled file as passing. All new portable cases must be enabled across the applicable matrix before the PR is complete.

### 2. Centralize aliases and export interface metadata

- [ ] Add one typed `compiler_aliases` registry with canonical spelling, semantic identity, arity, member owner, construction policy, and visibility/provenance rules. Inventory every primitive, container, and special-form alias rather than only media/string/json.
- [ ] Route source and precompiled-package lowering, display, `Self`, member lookup, pattern matching, and export through the registry. Preserve builtin identity and transparent user aliases. Add useful diagnostics for forbidden carrier spellings in all relevant source positions.
- [ ] Extend `crates/baml_ide/src/symbol_pool.rs` and `baml_codegen_types` from the compiler's `PackageInterface` semantics: interfaces, implementing types, associated pins, requirements, defaults, effects, method callability, and metadata-only field requirements.
- [ ] Preserve symbolic associated projections and scoped type parameters in the shared export model before native lowering. Add export tests for `Self.Output`, associated Error, method-generic Out, same-named parameters in different scopes, and out-of-body generic implementation rules. Assert semantic structure, not only printed names; valid projections must never take the current `Unknown` recovery path.
- [ ] Include out-of-body implementations, public factories, required dependency packages, recursive records/errors, and configured SDK export roots. Remove interface/impl-member omission and the stripped LLM `client` argument.
- [ ] Export out-of-body implementations for arrays, strings and other builtin types. Use a receiver representation that does not assume class storage; distinguish concrete array data projection from an interface view retaining the same BAML array.
- [ ] Compute data-record versus live-receiver projections recursively. A record containing a live child remains a copied record; a marker alone does not make a concrete DTO behavioral.

Acceptance: compiler and generator checks prove canonical identity, no duplicate public carrier models, correct exact pins and `Self` behavior, open interface matching, and complete provider/agent reachability. Update affected snapshots through insta deliberately.

### 3. Implement the shared reference and invocation protocol

- [ ] Implement one receiver ownership model for callbacks, concrete live facades, and checked interface views. Keep concrete type identity and the complete declared implementation set independent of the current view.
- [ ] Implement transactional encode/adopt/release, runtime/session generation checks, ABI/schema mismatch rejection, and existing-ref recognition before host adaptation. Exercise partial failure in nested aggregates.
- [ ] Add fixed and realized-generic method dispatch with exact argument/result/error validation and default bodies executing in BAML. Preserve legal concrete-`Self`, bound-method, and receiverless operations; expose no bridge field get/set or shared container storage operations.
- [ ] Preserve the full bound-method frame across specialization and host invocation. Reject a wrong argument before receiver execution, retain output-only type arguments for empty results, and validate live arguments by authoritative type identity. The focused engine tests now pass the fixed-specialization check; complete interface-view invocation and runtime-created receiver coverage remain required. See adjustment F10.
- [x] Carry caller argument layouts through MIR/bytecode and map them against the resolved target for virtual, indirect and native higher-order calls. Cover added/reordered implementation optionals, narrowed callable types, constant-call optimization, explicit runtime IDs and untouched receiver state on rejection. Do not infer stack argument count from the concrete callee. Verified across direct/indirect/virtual calls, native continuations, reflection, runtime IDs and constant-call optimization. See F12 and the results below.
- [ ] Retain the declared interface caller signature independently of the concrete implementation frame, including named defaults, exact associated pins, covariant Self results and declared errors. Check captured methods after releasing the original view and after GC; see adjustment F11.
- [ ] Implement automatic ownership, independent clone/local close, and active-call retention. Registry lookup/cache entries must not become permanent owners of all historical receivers in a singleton runtime.
- [ ] Implement optional scope revocation for callbacks and interface registrations, admission/close races, cancellation, actual-work drain, self-drain rejection, origin propagation, late-result disposal, and closed-reference diagnostics. No automatic arbitrary cross-runtime cycle collection or application resource disposal.
- [ ] Regenerate the affected `bridge_ctypes` schema consumers as one versioned cutover, including CFFI and Web paths. Preserve infrastructure failures without requiring generated application error classes to decode them.

Acceptance: deterministic shared runtime tests exercise ownership and protocol failures before broad backend rollout. Retention assertions inspect roots/leases and actual completions, not sleep-based timing or an assumption that host finalizers run promptly.

### 4. Complete the first vertical slice in Python

- [ ] Build a role-aware projection plan from the shared interface graph before rendering Python types. Preserve generic slot identity, associated declaration order, symbolic projections and required-interface substitution. Audit ordinary free-function/class symbol conversion too; it must not erase valid projections or silently strip supported interface parameters.
- [ ] Generate native annotations and decoder/dispatch registration together. Verify interface output produces the promised method-capable ref, retaining its issuing runtime and transactional ownership. Distinguish caller inputs from refs supplied to host bodies, recursively through callbacks and records.
- [ ] Generate and bridge a BAML-defined `Greeter`/`Counter`, its methods/defaults, and direct concrete-to-interface inputs. Prioritize using BAML implementations before host registration ergonomics.
- [ ] Round-trip the same receiver through multiple top-level calls; verify BAML-side method state changes and copied records with shared live children.
- [ ] Add native method-only implementations with lazy nominal registration, optional context, local calls on original objects, and explicit checked refs. Test ordinary classes and optional Pydantic models without construction-time runtime access.
- [ ] Add eligible ordinary generic adapters and the separately named advanced handler API. Test the runtime-created type path and declared errors.
- [ ] Exercise returned callable pass-back without re-registering it as a host callable or entering a nested blocking runtime. Cover the original image-callback use case in both directions, with portable image data retained beyond the callback/runtime.

Acceptance: the shared SDK fixture runs through generated Python APIs and the actual native bridge. Earlier isolated probes do not count as this result. Keep lifetime diagnostics/collection controls test-only where possible.

### 5. Port the stable contract through every active bridge

- [ ] Node TypeScript: ordinary implementation factory, checked refs, native adapter/handler distinction, recursive codecs, async calls, explicit disposal.
- [ ] Web TypeScript: same portable cases in Chromium and workerd, registry/cancellation/reentry/ownership tests on the actual Wasm bridge. No Node-only gating for portable behavior.
- [ ] Rust: typed generated refs and input roles, owned host implementations, eligible adapters/explicit handlers, dynamic values, media, errors, and clone/drop. Compile both supported and deliberately invalid API usage.
- [ ] Go: interface refs, method-generic caller helpers, checked native registration, errors and cancellation.
- [ ] C#: shared fixture execution, typed/dynamic adapters, disposal and managed/native ownership, trimmed-consumer checks where currently supported.
- [ ] Java: real JUnit execution, associated/generic tokens, owner-thread callbacks, retained references and release.
- [ ] Swift: async/concurrency requirements, dynamic values and generic fallback, platform-owned resources.
- [ ] C++: generated typed facades, handler registration, copy/move/RAII ownership, callback lifetime and cancellation.

Acceptance: shared cases execute in the ten parity environments. A language limitation changes the supported API shape or yields an explicit diagnostic; it must not silently delete BAML interfaces, provider clients, or callable methods. Ruby's loader exists, but it lacks the generated value/call SDK required for this feature; preserve loader compatibility during the ABI cutover and record that boundary, rather than claiming full Ruby parity.

### 6. Make media, provider clients, and agents work end to end

- [ ] Declare open `media`, the concrete payload union, and explicit `ImageSource` conversion. Keep concrete media portable and require no universal `snapshot()` or conversion during matching/encoding.
- [ ] Complete all concrete media codecs, nested aggregate/callback paths, URL/encoded-bytes/file-descriptor behavior, and no-implicit-I/O checks. Keep Pillow optional and test its adapter separately from portable semantics.
- [ ] Generate actual provider factories and implemented methods; accept clients directly in per-call overrides and streaming entrypoints according to their real interfaces.
- [ ] Extend `sdk_tests/fixtures/llm_functions/` and its recordings/transport harness with interface client/agent cases. Use a deterministic scripted transport; browser/worker tests use supported fetch mocks instead of local listeners.
- [ ] Execute `Agent.new(client=...)`, typed function specs, and `agent.run(spec)` with two output types. Check inferred result type, default-client versus override selection, journal methods, live nested callbacks, and native result fields.
- [ ] Test a fake host client and an advanced host runner, typed failure behavior, retry/fallback, streaming termination/cancellation, and retained clients/journals/streams across calls.

Acceptance: no credentials or paid model calls are required. Changing a function's client changes the provider actually used without changing the return contract or stored default. An image-only client does not acquire unsupported streaming capabilities.

### 7. Finish migration, matrix validation, and review

- [ ] Migrate canonical source spellings, generated docs/examples and consumers; update `TYPE_SYSTEM.md` and the journal design for the implemented rules. Remove obsolete exports/encodings and unused snapshots without compatibility shims.
- [ ] Run the complete relevant SDK crates after narrower slices pass, including setup guards and `build_diagnostics::no_build_failures`. Inspect ignored tests and port gates; compilation or declaration presence alone is insufficient.
- [ ] Run parity lint, inspect every new feature row for all required environments, and update its baseline only to record reviewed coverage. Overall improved totals do not excuse a missing new case offset by an unrelated fix.
- [ ] Run the repository snapshot and broader compiler/runtime checks, verify regenerated protocol files, and review namespace/API migration errors for mismatched artifacts.
- [ ] Add exact `.markdown-whitelist` entries for the design/guide/plan/log documents that will be committed; the validator only inspects tracked files, so an untracked document is not yet validated by that check.
- [ ] Reconcile internal design/guide examples with the generated APIs; update the adjustment log with final decisions and evidence. Record unsupported environments and unfinished work explicitly. Keep proposed BEP edits internal while the publication hold remains in effect.

Acceptance: all earlier gates have evidence, all intended new portable cases execute across the matrix, and the single PR has passing relevant checks for its current commit. A platform outage is not a passing test.

## Shared SDK cases to add

Use category `interfaces` and the following canonical names. Each is an executable behavioral case; parameterize values within it as needed. Keep identical names across Python `test_<name>`, TypeScript `test("<name>", ...)`, Rust/Java/Swift `test_<name>`, Go `Test_<name>`, C++'s existing macro convention, and the new C# per-case wrappers. The tool produces `interfaces/<name>`.

| Canonical case | What native assertions prove |
|---|---|
| `baml_receiver_roundtrip` | A BAML-returned interface has callable methods and can pass back without replacement. |
| `concrete_implements_interface_input` | A generated concrete implementor passes directly; no `.as_*()` step. |
| `default_method_dispatch` | A BAML default invokes the selected implementation and preserves receiver state. |
| `owner_state_survives_method_calls` | Add through the host and through BAML, then observe the same counter through an earlier reference. |
| `host_local_and_bridged_state` | Original host method calls and incoming BAML calls reach one host receiver. |
| `host_membership_is_explicit` | Declared implementation succeeds; unrelated same-shaped input is rejected. |
| `record_copy_preserves_live_child` | Editing copied data does not edit the source; methods on its live child affect that child. |
| `field_requiring_host_binding_rejected` | Native registration fails before creating roots; BAML-owned field-bearing receiver methods still work. |
| `marker_roundtrip_and_match` | Marker-only objects preserve concrete identity; no conversion is invoked. |
| `associated_types_are_exact` | Correct pins/defaults work, repinning and wrong result types fail. |
| `requirements_and_concrete_self` | Required views share identity; concrete-`Self` mismatches are rejected. |
| `generic_calls_infer_and_select_types` | Two output types on one receiver work; explicit tokens cover non-inferable cases. |
| `generic_handler_runtime_created_type` | Explicit handler round-trips a runtime-created type and nested live ownership. |
| `generic_contract_failure` | Invalid result/effect/type bounds fail without adopting leaked references. |
| `callable_and_interface_passback` | Returned callbacks/refs reuse their original capabilities, including inside records. |
| `media_callback_roundtrip` | Each concrete media kind crosses args/results both ways, including the B-1683 image scenario. |
| `media_is_portable_after_shutdown` | Media data/source descriptors remain usable without the originating runtime. |
| `media_source_conversion_is_explicit` | An adapter is not an image; conversion happens only when invoked. |
| `clone_close_and_retained_owner` | Releasing one SDK owner leaves independently retained owners usable. |
| `active_call_outlives_local_close` | In-flight work retains its receiver until actual completion. |
| `scope_revokes_and_drains` | New work is rejected and admitted work completes safely before teardown. |
| `scope_origin_survives_roundtrip` | Projection, nested returns and pass-back cannot escape the originating scope. |
| `scope_cycle_can_be_revoked` | An intentional host→BAML→host cycle is revoked/drained without relying on automatic cycle collection. |
| `singleton_releases_unowned_registrations` | Controlled release/collection drains roots with the runtime still open; bookkeeping is bounded. |
| `declared_error_and_cancellation` | Declared errors, wrong host returns, cancellation and closed/runtime errors remain distinct. |

Use the controlled runtime hook for the singleton case; do not depend on host-GC timing. Add language-specific finalization, Pydantic, `using`, Rust bounds, and compile-fail tests separately. Precise cancellation/adoption races, self-drain and partial-encode rollback belong in deterministic runtime tests as well as selected SDK cases. BAML `match` exhaustiveness, forbidden carrier syntax, coherence and illegal existential calls belong in compiler negative tests.

Add these canonical `llm_functions` cases: `interface_client_override`, `interface_streaming_client_override`, `interface_agent_run_typed_specs`, `interface_host_client_prompt_callback`, `interface_host_runner_generic_output`, and `interface_retry_failure_preserves_identity`. Mirror them into C# with the same shared-source/per-case mechanism. Alias export tests extend existing `type_shapes` where applicable; create matching C# discovery there for any newly shared case.

### Additional cases from type/codegen review

Add these canonical `interfaces/<name>` cases to the shared fixture and every applicable native bridge, using the existing declaration-parity and execution rules:

| Case | Assertion |
|---|---|
| `builtin_iterable_receiver_roundtrip` | Array and string implementors export through Iterable with exact Item/Error pins, invoke methods, and pass back without a class-only receiver assumption. |
| `array_copy_and_interface_view` | Concrete array results are copied; interface refs retain BAML state. Mutating a native data copy cannot change the retained BAML array. |
| `iterator_state_and_done` | Calls from both sides advance the same iterator; Done is distinct from a nullable Item. |
| `native_collection_requires_conversion` | Unadapted native collections fail before receiver execution; the explicitly typed BAML array factory works for empty and nonempty input. |
| `associated_error_bindings_are_exact` | Preserve multiple associated bindings, exact never defaults and declared error values across return, argument and callback paths. |
| `host_method_interface_crossing_directions` | A host body receives a callable checked ref with exact associated pins. It may return a matching ref, generated concrete facade or declared host source; completion checks/projection preserve identity and reject wrong pins while releasing provisional registrations. Native signatures must reflect the appropriate direction, including nested callbacks. |
| `required_interface_preserves_associated_bindings` | An Iterator with Error=ReadError is accepted as the matching Iterable; a never-pinned input rejects it before execution. Its `iter()` returns a ref preserving both pins and its later `next()` retains the declared error contract. |
| `host_required_interface_bundle_is_atomic` | A host implementation supplies the required interface bundle once. An invalid required member or binding publishes no usable registration and releases temporary ownership. |
| `interface_method_declared_contract` | A wider implementation input cannot widen checked calls. More specific implementation returns and legal Self returns preserve the declared interface view; captured methods and reflection agree. |
| `interface_method_named_optionals` | Required slots map by position and optional slots by name, with reordered/extra implementation optionals and implementation-owned defaults. Cover host calls, pass-back and native higher-order invocation through narrower function types. |
| `bound_method_preserves_specialization` | Retain a supported method handle across collection and pass-back; preserve default-owner and method arguments for class and primitive receivers. An output-only generic result retains its exact type when empty. |
| `specialized_call_rejects_wrong_argument` | A fixed string specialization rejects an integer before executing the receiver, then still accepts a valid call. Exercise the checked dynamic path where native typing prevents the invalid call. |

Compiler/export tests additionally check non-class generic implementation substitution, reachability through required interfaces, and diagnostics naming an unsupported member. Native compile/type-check fixtures must cover both accepted generated concrete inputs and intentionally wrong bindings; runtime tests use the dynamic entrypoint to exercise rejection in statically typed languages. Keep negative BAML source out of the valid shared SDK fixture. A declared test that never executes is not evidence for any of these cases.

Extend `native_collection_requires_conversion` to empty and nonempty native lists with the same interface input. Neither may infer a new registration from current elements or associated pins. Verify the typed-data factory and an explicitly declared host adapter separately: one copies into BAML, the other retains the host receiver. In the backend review table, track calling a member separately from implementing it with an ordinary method or advanced handler.

Strengthen existing cases instead of adding duplicate coverage names:

- `associated_error_bindings_are_exact`: hold Output constant and vary only Error. Check native assignment rejection where expressible and dynamic admission rejection in every bridge. An unused host-protocol parameter must not erase the ref/input binding.
- `host_required_interface_bundle_is_atomic`: use a host call counter to prove binding never invokes a user body, including failed binding. Assert provisional registrations and nested leases are released.
- `specialized_call_rejects_wrong_argument`: assert the receiver call counter stays unchanged on admission failure, then make a valid call using the same ref.
- `declared_error_and_cancellation`: for a wrong host result/error, assert the host body ran once, its owner-language side effect remains, the invalid completion never reaches the BAML continuation, and rejected result leases are released. Result validation is not a transaction over user state.

These are planned assertions, not newly executed tests. Keep compiler/native negative cases separate from the valid shared fixtures, and record generated caller support separately from ordinary host-adapter support.

## What the existing parity tools guarantee

- `../tools/sdk-parity-lint/run` performs deterministic AST declaration discovery. It covers Python, Node TS, Chromium TS, workerd TS, C++, C#, Rust, Go, Java and Swift. It does not run tests, compare assertion bodies, or discover Ruby loader tests.
- IDs are exact `<fixture>/<canonical_name>`. Python declarations provide the percentage denominator, but the ratchet also checks required gaps, declaration deletion, and weakened requirements. Existing baseline gaps remain allowed.
- `SDK_PARITY_LINT(skip): <reason>` waives absent environments for that canonical ID, not just one local test. Use it only for genuinely language-specific behavior; never to conceal an unfinished portable implementation.
- TypeScript runtime gates affect the required environment set. Keep portable cases ungated.
- CI's `sdk-test-coverage` job only verifies matrix entries for SDK directories. Keep both this coverage gate and the case-level parity gate, plus native execution.

The C# fixture/category change and Rust execution-gate issue are prerequisites in checkpoint 1, not reasons to claim parity from current reports.

## Validation commands and CI fallback

Run every Cargo command from `baml_language/`. Use `cargo nextest run`; only doctests may use `cargo test --doc`. Check actual test names with `cargo nextest list` after generation. New fixture filters below become valid when the fixtures land.

```sh
cargo nextest run -p sdk_test_python_pydantic2 -E 'test(/interfaces::|build_diagnostics::|setup_guard::/)'
cargo nextest run -p sdk_test_typescript -E 'test(/interfaces::|build_diagnostics::|setup_guard::/)'
cargo nextest run -p sdk_test_typescript_web -E 'test(/interfaces::|build_diagnostics::|setup_guard::/)'
cargo nextest run -p sdk_test_rust -E 'test(/interfaces::|build_diagnostics::|setup_guard::/)'
```

Run the equivalent enabled fixture checks for `sdk_test_go`, `sdk_test_csharp`, `sdk_test_java`, `sdk_test_swift`, and `sdk_test_cpp` on supported hosts. The C# wrapper filters will be fixed during harness work; confirm discovery instead of assuming those Rust module names already exist. Run full relevant SDK packages before completing the PR. A filtered run omitting setup guards/build diagnostics cannot establish that generation and setup succeeded.

From this same directory, the parity command is:

```sh
cargo build -p baml_cli --bin baml-cli
../tools/sdk-parity-lint/run --repo-root .. \
  --baseline sdk_tests/crates/parity_analysis.md \
  --output sdk_tests/crates/parity_analysis.new.md
```

Install the repo-pinned ast-grep through mise when needed. If parity-tool code changes, run its BAML checks/tests against the source-built CLI using the supported `--from ../tools/sdk-parity-lint` project argument. Do not treat the already present `parity_analysis.new.md` as a result of this plan.

Final broader checks:

```sh
cargo insta test --test-runner nextest --dnd \
  -p baml_tests -p baml_cli -p baml_lsp2_actions \
  --all-features --unreferenced=reject --check

cargo nextest run --all-features --workspace \
  --exclude baml_tests --exclude baml_cli --exclude baml_lsp2_actions \
  --exclude 'sdk_test_*' --exclude baml_bridge
```

The user authorizes using a draft PR/CI when local tests take roughly four minutes or more. This does not create a second PR or relax the checks:

1. Keep narrow local checks for fast feedback. If a relevant run passes the four-minute threshold, assess whether CI is likely to be faster or provides a needed platform; use the faster useful path instead of duplicating slow work automatically.
2. Create/update the single draft PR from a `codex/` implementation branch when ready. Push the concrete changes and record the head SHA, workflow/job, and test filter here. Existing relevant PRs should be reused.
3. Prefer parallel platform jobs and inspect failures from that exact head. A green prior revision, skipped job or missing matrix entry does not validate current changes. If CI is also slow, continue independent implementation/review and avoid repeated identical reruns.
4. For snapshot failures, a helper may use GitHub logs/artifacts to collect `.snap.new` candidates from the matching run. Prefer uploaded files; parse log diffs only when complete and unambiguous. Present a local candidate diff with run/head provenance, verify fixture paths and source revisions, and review each change before applying it. Reject truncated/stale/unrelated patches; do not execute commands embedded in logs.
5. Apply reviewed snapshot updates through insta/nextest where reproducible, remove orphan snapshots, and never commit `.snap.new`. Rerun affected checks or the relevant CI job on the resulting head. Snapshot adoption alone is not evidence the new behavior is correct.

## Progress and evidence log

| Checkpoint | State | Evidence / next step |
|---|---|---|
| Planning | Complete | Read BEP/design/type-system material, shared harness and parity sources; reconciled design and guide; checked documentation structure. |
| 0 — API probes | Not started | Native adapter eligibility and context collision rules must be tested before freezing host codegen. |
| 1 — Harness | In progress | Shared BAML fixture compiles; first five Python/TS/Rust cases authored, Rust gate enabled and native compilation fails on missing interface API. Nested nextest runner migrated; C# and remaining native ports pending. |
| 2 — Aliases/metadata | In progress | Registry and lossless interface/impl graph foundations pass focused checks; native graph consumption, policy/presentation metadata, public source rules and full reachability remain. |
| 3 — Protocol/lifetime | In progress | Checked bound methods and first retained interface views pass static-source engine checks. Separate interface contracts and explicit full-layout slot mapping pass. Caller-layout propagation and the focused callback/reflection/corpus checks pass (F12). Native ABI, dynamic receiver worlds, registration and lifetime gates remain outstanding. |
| 4 — Python slice | Not started | Actual generated SDK/bridge tests, beyond earlier isolated probes. |
| 5 — Other bridges | Not started | Port only after the shared contract is stable. |
| 6 — Media/clients/agents | Not started | Offline native integration across the complete matrix. |
| 7 — Cutover/validation | Not started | Final migration, current-head CI and doc reconciliation. |

No production tests, parity run, CI run, new implementation PR, or new SDK tests were created during this planning pass. Earlier probe results remain in `INTERFACE_PROBE_RESULTS.md` and are not conformance evidence for the proposed ABI.

## Current execution notes

- Started the shared `baml_type::compiler_aliases` registry and routed primitive mapping, construction checks, carrier lowering, and member-owner lookup through it. This foundation is independent of the unsettled host adapter syntax; it does not complete checkpoint 2 or the public carrier-name ban.
- Narrow verification: `cargo nextest run -p baml_type -p baml_compiler2_hir_ty -E 'test(/compiler_alias|primitive::tests::|alias_tests::|type_kind::tests::/)'` passed 16 tests. Compiler integration, codegen snapshots, and native SDK behavior remain unverified.
- Latest BEP feedback: prepared local revisions explaining caller versus incoming host context and immediate result shapes. No BEP push was made. Keep the website unchanged while the user supplies further feedback.

- Review follow-up recorded in adjustment A6: expand associated-type codegen examples to Python/TS/Go/Rust and pair caller/implementer signatures. Add acceptance coverage for BAML arrays and strings returned through `Iterable`, exact Item/Error pins, live pass-back versus copied array results, and rejection of unsupported/untyped native collection adaptation. Keep new portable case IDs aligned across bridges. No website update pending review.

- Internal feedback revision completed: design §§6.1.1, 9.2.1 and 9.14 now specify non-class receivers, associated-type/error projections, and failure stages. The guide now introduces the generated OpenAI facade, paired associated-type signatures including Go, array/interface ownership examples, explicit GenericCall types and snake_case authored functions. No production source or BEP files were changed in this documentation pass.

### Implementation evidence — alias foundation and first shared cases

- Added `sdk_tests/fixtures/interfaces/baml_src/main.baml`: BAML-owned Greeter/Counter, copied records with live children, associated Output/Error, generic Echo, array/string Iterable, nullable iterator, media callback and callable pass-back. Source-built CLI `check --project sdk_tests/fixtures/interfaces/baml_src` passed.
- Added five matching native cases in Python, canonical TS (also used by Web), and Rust: receiver round-trip, concrete interface input, default dispatch, owner-state method calls, and copied record/live child. Rust is `Gate::Now`; generated `tests/main.rs` includes it. Other language ports and the remaining planned cases are still required.
- `cargo nextest list -p sdk_test_rust -E 'test(/interfaces::|build_diagnostics::|setup_guard::/)'` confirms fixture discovery and the renamed `interfaces::nextest` runner. Native `cargo nextest list --manifest-path sdk_tests/crates/rust/interfaces/generated/Cargo.toml` (shared SDK target cache) fails with missing interface entrypoints/methods, as expected before interface export/codegen. This is a failing acceptance test, not completed bridge support. Generated diagnostics also report existing unsupported symbols.
- Migrated nested Rust test execution and Unix/Windows prewarming to nextest. A structured Cargo helper pins the fixture manifest, runs from the BAML workspace, keeps the shared target directory and engine env, and handles paths with spaces. One focused harness-runner test passed; `bash -n` passed. Windows execution and an actual successful native fixture run remain outstanding.
- Rebuilt `baml-cli` successfully in 53 seconds. Existing media corpus filter passed 35 tests. New compiler alias runtime cases passed 2 tests: all concrete media factories/methods through canonical spellings and array-to-Iterable dispatch. These runs preceded the latest AST/static-qualifier cleanup and will be repeated after that change.
- AST source-name lowering now uses `compiler_aliases`, rejects builtin arity/associated-binding mistakes instead of discarding arguments, and keeps map/json construction there. Static qualifier inference also uses the registry. Added the diagnostic fixture `compiler_alias_type_arguments`; snapshot verification is running. Public carrier-name prohibition and migration remain unimplemented.

### Verified at the end of the alias/harness slice

- Rebuilt the CLI after all AST/static-qualifier edits. `target/debug/baml-cli test --from crates/baml_tests/baml_src -i compiler_alias_ -i media_` passed **37/37** tests in 55 seconds.
- `cargo nextest run -p baml_type -p baml_compiler2_hir_ty -p sdk_test_harness_runner -E 'test(/compiler_alias|primitive::tests::|alias_tests::|type_kind::tests::|rust_fixture_tests/)'` passed **17/17** selected tests.
- `cargo insta test --test-runner nextest --dnd -p baml_tests --accept -- compiler_alias_type_arguments` passed **3/3** tests. Reviewed the final snapshots: exactly one error per invalid annotation, with no misleading parse-error cascade.
- The `corpus_snapshots` insta/nextest pass passed. Reviewed all five changed existing snapshots as text: builtin media carrier types become concrete media types in receiver signatures, matches, generic arguments and prompt/media records; the root adds the new test namespace registration. Added that namespace's bytecode snapshot. No `.snap.new` files remain.
- `cargo fmt --all -- --check`, `git diff --check`, and the Unix setup script syntax check passed. Five authored native case IDs agree across Python/TS/Rust; this small source check is not the full parity lint or proof of native execution.
- Outstanding: full interface metadata export and all bridge implementations, remaining native tests/parity wiring, public carrier-spelling ban/codemod, native Rust green run, Windows setup validation, and the single implementation PR. The native interface tests remain deliberately enabled and failing on missing generated interfaces. No BEP update or CI run was made.

### Implementation evidence — shared interface export

- `baml_codegen_types::SymbolPool` now carries an `InterfaceGraph`, populated by `baml_ide` from compiler `PackageInterface` declarations and implementation rules. It includes source-less packages, exact associated pins/defaults/bounds, scoped generic slots, requirements, required/default methods, field metadata and symbolic call targets. Non-class receivers retain their actual type patterns. Native signature rewrites preserve the graph.
- The graph uses the existing `RuntimeTy` family to retain associated projections and reject nested compiler recovery states. `build_symbol_pool` now returns an export error with the affected declaration/member instead of inventing an opaque type for invalid interface metadata; CLI and shared/C# harness callers handle that result.
- This exposed and fixed a missing owner/bound scope in `callable_throws`, which had exported a compiler error for `Iterator.collect`'s `Self.Error`. Tests cover both that default and a method-owned bounded type parameter's Output/Error projections. See adjustment F9.
- `cargo nextest run -p baml_ide -p baml_codegen_types -E 'test(/symbol_pool::|symbols::tests::/)'`: **32/32 passed**. New cases cover the shared SDK fixture, stdlib array/string implementations, required-interface pins, generic method frames, recovery rejection and source-less stdlib export. The source-less test requires no source fallback and verifies the compiler's specific trusted-builtin linkage upgrade.
- `cargo check -p baml_cli -p sdk_test_harness_setup --all-targets`: passed in **1m 59s**, including compilation of the generator dependencies. This is a Rust integration check, not native SDK execution.
- `cargo insta test --test-runner nextest --dnd -p baml_tests --check -- corpus_snapshots`: passed; **no additional snapshot changes** from this slice.
- After making the semantic graph shared and immutable through `Arc`, reran the export/shared-symbol selection: **9/9 passed**. A source-built CLI run of `associated_type_projection_in_throws_position_runs`, `default_method_self_call_yielding_associated_type_runs`, `own_generic_bound_after_associated_type_reaches_default_body`, and `alpha_equivalent_method_generics_and_bounds_run` passed **4/4** in 52 seconds. These exercise the actual BAML runtime after the callable-effect scope fix; they do not establish native interface support. Formatting and whitespace checks passed.
- Still required before checkpoint 2 is complete: generator naming/reachability over the graph, source presentation and implementation-policy metadata, exact native method projections, class factories and implemented methods, client argument restoration, and the public canonical-spelling migration. The new graph does not yet make the native interface acceptance tests pass. No BEP, website, PR or CI update made.

### Review checkpoint — existing bound-method work

- Existing edits in MIR lowering, VM specialization and engine callable entry preserve interface dispatch and the complete bound-method frame. They are groundwork, not an implemented interface-view ABI or generated SDK surface.
- Last recorded `cargo nextest run -p bex_engine --test interface_handles`: **1 passed, 1 failed**. The stateful default-method case passes across major GC and pass-back. The strengthened generic case fails because `echo<string>` accepts `Int(9)`; its later correct-string and output-only empty-array assertions are not reached in that run. An earlier weaker 2/2 result does not establish the stronger contract.
- The new `generic_method_values.baml` corpus cases still need compilation, runtime execution and reviewed snapshots. No fresh implementation tests were run in this documentation-only review.
- Next implementation action: validate arguments against the retained realized signature before body execution, including live type identity and exact pins; then run the focused engine and corpus checks. Keep all native bridge gates open. See adjustment F10.


### Implementation evidence — checked bound-method invocation

- Fixed interface-default metadata to include the implicit Self slot before interface and method parameters. It previously exposed only the method parameter name, so a complete captured `[Self, T]` frame still reconstructed T as `unknown`. MIR dispatch, VM specialization and engine invocation now preserve the same frame. Removed the engine's redundant class-only return-type reconstruction.
- Callable entry now checks converted arguments against the original anchored runtime signature before execution. The VM uses singleton-precise canonical membership, so literal types, invariant class arguments and exact associated pins are checked. Diagnostic handle annotations cannot repin an object. Opaque native payloads are recognized only through their actual RustData representation and explicit internal `$rust_type` contract, rather than exempting all handles from checking.
- Added five engine-boundary tests in `crates/bex_engine/tests/interface_handles.rs`: retained default dispatch/state/pass-back across GC; primitive receiver and output-only method specialization; separate class/method parameters; rejection of forged generic-class pins, incorrect interface associated pins and a wrong literal before state changes; and explicit opaque host-value round-trip. The checked-state case also asserts that all external roots return to the baseline while the engine stays open.
- `cargo nextest run -p bex_engine --test interface_handles --test fn_typed_slots --test host_value_callable --test identity --test function_resolution --test random`: **72/72 passed**, with four nextest output-leak reports. Reran those four specific existing cases with `-j 1`: **4/4 passed without leak reports**. This is runner evidence, not a proof of general memory boundedness.
- After strengthening associated-pin coverage and adding opaque payload handling, `cargo nextest run -p bex_engine --test interface_handles --test host_value_callable -j 4`: **36/36 passed**, one existing test skipped, no leak reports. This includes all five new engine cases.
- Built the CLI and formatted `generic_method_values.baml`. `target/debug/baml-cli test --from crates/baml_tests/baml_src -i generic_interface_method_value`: **3/3 passed** in 77 seconds, covering independent specializations, empty typed results and field-access receivers.
- `cargo insta test --test-runner nextest --dnd -p baml_tests --check -- corpus_snapshots` found the expected new test registration. The subsequent `--accept` run passed. Reviewed the root and `ns_interfaces` bytecode changes: the added fixture registration and its generic default bodies only, alongside the previously reviewed alias changes. No `.snap.new` files remain.
- Still required: first-class checked interface views and concrete live facades, exact type/declaration ownership across GC, receiver-world-aware validation for runtime-created implementations, direct method ABI, native codegen and all portable SDK cases. The existing VM type context falls back to the lexical runtime package; do not claim dynamic receiver-world coverage from static-source tests. No BEP, website, PR or CI changes made.


### Implementation evidence — retained interface views (in progress)

- Added an engine-retained interface ADT with inline primitive or rooted heap receiver, exact interface arguments/pins, rooted type declarations and an optional implementation-world root. Projection/adoption checks runtime provenance and exact conformance. The common CFFI schema has an interface handle tag; Rust/Python generated schema files were refreshed. Native interface wrappers and direct method entrypoints are still missing.
- Added compiler-derived method callability and complete associated-member names to runtime interface metadata. A checked method now stores its declared interface function contract separately from the callee's concrete generic frame. Argument conversion/validation and result/error projection use that caller contract. Both interface metadata and bound-method contracts participate in type-head GC tracing. Artifact format 3 and cache format 12 invalidate older stored layouts.
- `cargo nextest run -p bex_engine --test interface_views -j 4` passed **6/6** tests before adding the CFFI-transfer and rejection cases. Covered class/primitive/array receivers, defaults, state, copied aggregates with shared live children, capture and GC, foreign-runtime rejection, interface input/output variance, covariant Self, and associated/interface/method parameter separation.
- Additional checks are in progress for CFFI ownership transfer, incomplete/incorrect/duplicate associated bindings and concrete-Self-only operations. The complete new test selection, affected existing tests, GC walkers, schema checks and snapshots still need a final result recorded below. No native bridge, parity or CI success is claimed.

- Follow-up verification: all eight retained-view tests passed, including CFFI transfer/last-release and rejection of missing, wrong and duplicate associated bindings. Callback compatibility testing exposed copied-interface expectations in arguments, returns and throws; updated those cases to assert live views, release transferred ownership, call error methods and prove concrete identity on pass-back.
- `cargo nextest run -p bex_heap -p bex_cache -p baml_artifact -p bridge_cffi -E 'package(bex_cache) | package(baml_artifact) | test(/bound_interface_contract|interface_owner_and_default|impl_rule_edges|api::tests::/)' -j 4`: **25/25 passed**. Includes a moving-GC test where the only path to a declaration and its package is the retained method contract.
- Added shared engine/VM slot mapping after checking the canonical optional-parameter superset rule. The new case tests reordered optional parameters, extra implementation defaults, hidden-option rejection and checked-callable pass-back. A first fixture used unsupported signature-default syntax and failed compilation; it has been corrected to use a default method. Its rerun and corpus snapshot verification are pending.

- `cargo nextest run -p bex_engine --test interface_views --test interface_handles --test host_value_callable --test fn_typed_slots -j 4 --no-fail-fast`: **48/49 passed**, one existing test skipped; the optional fixture's syntax was the only failure. After correcting that fixture to a default method, its isolated nextest rerun **passed** and verified named host calls plus full-layout captured-callable pass-back.
- `cargo insta test --test-runner nextest --dnd -p baml_tests --check -- corpus_snapshots`: **passed**, build 1m33s plus 12s test execution; no new snapshots to review.
- A further legal-subtyping probe is now enabled in `interface_views.rs`: `checked_method_narrower_callable_and_native_arguments`. At that checkpoint it **failed** in all three collected scenarios: direct virtual call reports expected 4/got 3 arguments; narrowed indirect call delivers a closure instead of a string; native `Array.map` rejects the reduced argument list. This is a runtime dispatch gap, not invalid fixture syntax. See F12 for the required caller-layout design and next implementation task. Do not report the new interface suite as green while this case fails.

- Final compile check for this slice: `cargo check -p bex_engine -p bridge_ctypes -p bridge_cffi --all-targets` **passed** in 1m14s. `cargo fmt --all -- --check` and `git diff --check` passed. At that checkpoint F12 still failed behaviorally; the continuation below records its correction. No BEP/website edit, PR, push or CI run was made.


### Caller-layout implementation continuation

- Introduced one `CallLayout` mapping rule across checked interface entry, direct/indirect/virtual bytecode dispatch and native continuations. Call layouts are serialized semantic data, not debug metadata; type arguments and runtime IDs retain separate lanes.
- Removed synthesized optional-parameter adapter closures. A new source/bridge comparison caught their missing optional metadata; dispatch now maps the original function without replacing its identity. Added an explicit positional layout for spawn transformers, which are compiler-synthesized calls of user callables.
- First focused rerun: `cargo nextest run -p bex_engine --test call_layouts --test interface_views -j 4`: **11/11 passed** (27s build, 10s execution). Includes ordinary function handles, reflection, added/reordered optionals, source coercions, native map continuation and fixed generic specialization.
- Strengthened these tests with explicit runtime IDs and reflected checked interface methods; broader engine checks and bytecode serialization/compact-PC tests are pending. The first broader command named `reflect_call_any` in the wrong package and did not run; that target belongs to `baml_tests`.


### Verified caller-layout correction

- `cargo nextest run -p bex_engine --test call_layouts --test interface_views --test interface_handles --test host_value_callable --test fn_typed_slots -j 4 --no-fail-fast`: **51/51 passed**, one existing test skipped (28s build, 44s execution). The strengthened fixture declares the `InvalidArgument` effect introduced by explicit runtime IDs. Its two earlier fixture-only compilation failures were corrected before this complete rerun.
- `cargo nextest run -p baml_type -p bex_vm_types --lib -E 'test(/call_layout/)' -j 4`: **3/3 passed**. Covers valid/invalid slot maps and preservation through Borsh serialization and compact instruction-PC translation.
- `cargo nextest run -p baml_tests --test reflect_call_any --lib -E 'binary(reflect_call_any) | test(/records_function_adapter|call_runtime_id/)' -j 4 --no-fail-fast`: **35/35 passed**. One nextest output-handle leak report on `call_any_bound_method_callee`; its isolated `-j 1` rerun passed without that report. This runner result is not a claim about general interface lifetime completion.
- `cargo insta test --test-runner nextest --dnd -p baml_tests --accept -- corpus_snapshots`: **passed** (1m31s build, 12s execution). Reviewed exactly three newly changed snapshots: fixture MIR and two optional-function bytecode snapshots remove captured adapter closures and call the original function directly. No `.snap.new` files remain. Earlier media/interface snapshot edits are unchanged by this continuation.
- Indirect calls now require explicit caller layouts in both the emitter and VM. The host-callable entry trampoline supplies its layout. The ordinary-function engine test explicitly checks that constant-call optimization retains the narrower layout, then executes it.
- `cargo check -p bex_engine -p bridge_ctypes -p bridge_cffi --all-targets -j 4`: **passed** in 1m05s. Formatting and whitespace checks passed.
- Next: complete runtime-created declaration identity and receiver-world-aware checking/dispatch before the native interface method ABI and SDK vertical slice. The shared SDK fixture remains an unfulfilled generated-client acceptance gate; these engine tests do not establish cross-language parity. No BEP, website, PR, push or CI changes.


### Runtime-created interface continuation (in progress)

- Added `runtime_created_receiver_retains_implementation_and_passes_back`, using the installed project runtime compiler and a real mounted app package. The initial helper lacked a runtime compiler; after correcting that setup, the test exposed E0099/E0115 from missing interface methods and a generated method namespace shadowing the interface.
- Fixing mount link stubs before receiver-world dispatch: required/default methods stay inside the interface declaration, with relocated targets and their checked signature shape. See F13. No passing dynamic-interface result is claimed yet.

### Runtime-created interface continuation — further evidence

- Fixed runtime package function resolution at callable entry and retained exact output contracts on GC-traced root threads. Added exact interface projection for direct results/errors and nested containers/record fields. Two compilations with identical declaration names preserve different interface identities.
- The runtime-created receiver and strengthened dynamic output tests passed **2/2**. The mounted associated/generic method test passed separately. `cargo nextest run -p baml_tests --test runtime_package_compile -j 4 --no-fail-fast` passed **20/20** (22.54s build, 11.89s execution).
- A broader engine selection (interface views/handles, layouts, callbacks, function slots, explicit/inferred generics) passed **131/132**, with two existing skips. The only failure was the newly added GC-during-host-await case, rejected before dispatch because callback binding widened explicit `throws never`. Its focused rerun passed after preserving never; the new unexpected-native-throw test also passed. The omitted-effect test then exposed incomplete emitted signature metadata; see F14.
- In progress: canonical elaborated function metadata, correct runtime layout for erased implicit effects and final affected engine/compiler checks. Do not count this checkpoint complete before those checks pass. No native bridge/codegen, SDK parity, PR or CI success is claimed.

### Verified runtime-created output and callback-effect correction

- `cargo nextest run -p bex_engine --test interface_views --test host_value_callable -j 4 --no-fail-fast`: **46/46 passed**, one existing skip, after the final metadata/layout correction (40.76s execution). Includes retained runtime declaration identity in nested outputs/errors, mounted associated/generic defaults, explicit never acceptance/violation, omitted native effects, and major GC while a host callback is suspended.
- `cargo nextest run -p bex_engine --test interface_handles --test call_layouts --test fn_typed_slots --test generics_inference --test generics_explicit -j 4 --no-fail-fast`: **87/87 passed**, one existing skip (74.87s execution), before the final generated-init return and runtime-layout metadata refinements. `cargo nextest run -p bex_engine --lib -E 'test(/conversion::/)' -j 4`: **83/83 passed**.
- Function metadata now uses the canonical elaborated signature, with `RuntimeGenericLayout` deciding which parameters occupy runtime slots. Compiler-only implicit effects are currently erased to unknown; they do not create phantom frame slots. Explicit never remains never. Generated init/test helpers retain their unit result. The previous authored-type re-lowering machinery was removed.
- `cargo insta test --test-runner nextest --dnd -p baml_tests --lib --accept -- corpus_snapshots`: **passed** (26.58s build, 12.56s execution), after diagnosing and fixing the generated-helper case. Reviewed 48 newly updated bytecode snapshots: omitted callback effects use unknown, and interface receiver signatures use their Self slot. Existing alias/caller-layout fixture edits remain in the worktree. No `.snap.new` files remain.
- `cargo check -p bex_engine -p bridge_ctypes -p bridge_cffi --all-targets -j 4`: **passed**, 44.65s. Formatting and whitespace checks passed. Final reflection/runtime-package and shared export checks are being collected below.
- Next implementation gate: carry exact contracts through inbound dynamic interface values and type arguments, including same-spelled declarations and partial-adoption failure. Then complete native method entrypoints, ownership/scope protocol, generated facades/host implementations and all portable SDK cases. These engine results do not close the shared reference protocol or native bridge checkpoints. BEP, website, PR, push and CI remain untouched.

- Final checks for this continuation: `cargo nextest run -p baml_tests --test runtime_package_compile --test reflect_call_any -j 4 --no-fail-fast` **54/54 passed** (24.17s execution). `cargo nextest run -p baml_ide --lib -E 'test(/interface_export/)' -j 4` **4/4 passed**, including source-less stdlib export and bounded method frames. The final formatting/whitespace check passed. The next gate remains exact inbound contracts followed by the native reference/method protocol; the goal is not complete.

### Current feedback continuation — internal docs only

- Added [the codegen review note](INTERFACES_CODEGEN_REVIEW.md) with paired associated Output/Error projections, generated implementation versus host implementation roles, invocation/context types, and non-class receiver failure stages. Updated the technical design and guide for optional Python host context and exact dynamic associated tokens. See A8.
- Kept native compile checks and bridge execution open. Proposed signatures are not generated-code evidence. No implementation edits, Cargo/native tests, BEP/site edits, PR, push or CI run in this documentation pass.
- Carried-forward implementation status: exact inbound conversion is unfinished. The latest recorded run of `runtime_interface_inputs_use_exact_declarations_before_execution` fails on a non-null `Peer?` argument during wire-only union pre-coercion, after its direct/array/map/record cases. The earlier 56/56 selection predates the strengthened test and final callback-signature edits. See F15; do not report the present interface suite as green.
- When implementation resumes, fix that focused failure first, rerun the affected exact-input/callback cases, and then proceed to the native method/reference protocol. Keep the new feedback acceptance gates alongside the existing parity and lifetime gates.


### Exact inbound contract continuation (in progress)

- Moved callable argument coercion into exact runtime conversion. Normalize associated projections before choosing storage, preserve exact class/enum/element types recursively, and validate retained interface/heap values against the actual VM contract rather than their wire names.
- Added allocation-free runtime union selection that recursively checks live children in copied arrays/maps/records. It retains the bridge ambiguity policy while comparing exact declarations. No speculative receiver execution or host registration is used to choose a union arm.
- The strengthened `runtime_interface_inputs_use_exact_declarations_before_execution` passes, including nested nullable children, array unions, optional records, same-spelled declarations from another compilation, unchanged receiver state on rejection, GC and external-root release. Focused nextest run: **1/1 passed**, 12.09s build plus 3.04s execution.
- The broader unit/integration selection initially failed to compile four existing peel-function helper tests after that helper became generic over declaration heads. Added explicit wire RuntimeTy annotations to those tests; the rerun is in progress. No full-suite or native SDK success is claimed yet.


### Exact inbound contracts and retained host-callable continuation

- After fixing the helper test annotations, the affected engine selection passed **169/169**, one existing skip (15.56s build, 53.56s execution). This was before the new captured-host-callable case and output conversion fix.
- Added `runtime_interface_callback_arguments_keep_exact_declarations`. Its direct/nested callback path passed first; capturing and returning the callable then exposed CannotConvert for HostClosure, followed by an unknown user.Named lookup when called outside the dynamic package frame. The corrected case passes **1/1** (20.83s build, 2.91s execution). It checks GC, later interface method use, exact direct/nested view identity and external-root release. See F16.
- Export HostClosure through the ordinary retained callable lane. Preserve exact required and optional host-call parameter types until their values are converted; wire names no longer select their interface declarations. The final combined engine run and all-target bridge compilation check are in progress.
- Remaining gates include annotated/dynamic-alias inputs, exact named generic-call inputs and GC-rooted host-call result/error contracts, followed by native interface entrypoints/codegen, shared SDK execution and ownership/scope protocol. Existing engine proofs do not establish those gates.

### Final verification for this continuation

- `cargo nextest run -p bex_engine --lib --test interface_views --test interface_handles --test host_value_callable --test fn_typed_slots --test call_layouts -j4 --no-fail-fast`: **170/170 passed**, one existing skip (15.41s build, 56.53s execution). This includes the final captured-host-callable fix and all new inbound assertions. No snapshots changed in this continuation.
- `cargo check -p bex_engine -p bridge_ctypes -p bridge_cffi --all-targets -j4`: **passed**, 45.20s. `cargo fmt --all -- --check`, `git diff --check`, and internal-document whitespace/fence checks passed.
- The original F15 optional-interface rejection and F16 captured-callback failures are corrected. The wider exact-contract/bridge checkpoints remain open: sparse annotated inputs and dynamic aliases, named generic input conversion, host completion/error contracts across suspension, transactional ownership/scopes, native generated facades/implementations and all portable SDK cases. No BEP, website, PR, push or CI update.

### Exact host completion contracts — F17

- Callback result/error contracts now remain GC-traced on root and child threads while the host call is suspended. Completion uses the forwarded exact declarations. Removed the engine's interface/unknown return blacklist while retaining unresolved-generic rejection.
- Added a runtime-created interface callback test covering correct and same-named foreign interface returns/throws after major GC, later method calls and return of external roots to baseline while the engine remains open. Expanded it with copied Packet fields to reject an integer supplied for a float and a foreign live child before input coercion can mask the error.
- Added explicit `unknown`, valid/malformed SDK panic under `throws never`, and JSON finite/non-finite completion cases. The panic probe initially caught an incorrect contract violation for a valid panic; panic payloads now use their own verified declaration schema. A small Rust borrow-signature compilation error in the JSON guard was fixed before its passing test.
- `cargo nextest run -p bex_engine --test host_value_callable -E 'test(/host_panic_is_separate|runtime_interface_callback_completions|declared_unknown_is/)' -j4`: **3/3 passed**, 3.41s execution. `host_completion_preserves_json_payload_restrictions`: **1/1 passed**, 11.40s build plus 2.84s execution.
- `cargo nextest run -p bex_engine --lib --test host_value_callable --test interface_views --test interface_handles --test fn_typed_slots --test call_layouts -j4 --no-fail-fast`: **174/174 passed**, one existing skip (9.49s build, 57.89s execution). `cargo check -p bex_engine -p bridge_ctypes -p bridge_cffi --all-targets -j4`: **passed**, 36.08s.
- A final test-only extension invokes the same suspended callback from a BAML-spawned child, covering both thread ownership paths. `cargo nextest run -p bex_engine --test host_value_callable -E 'test(/runtime_interface_callback_completions/)' -j4`: **1/1 passed**, 5.91s build plus 3.05s execution. The combined run above predates that extension; production code is unchanged by it. Final formatting and whitespace checks passed. No snapshots changed in this continuation.
- Remaining gates: sparse annotated/dynamic-alias inputs, exact named generic input conversion, native interface entrypoints and generated APIs, transactional ownership/scopes, and portable SDK execution. This engine/test-dispatcher verification does not close those checkpoints. The BEP and website remain unchanged; no PR, push or CI run.

### Named generic input continuation — in progress

- Generalized shared inference over declaration heads and caller-supplied type facts. Named runtime calls now use exact VM types, including interface children of copied arrays/maps, and honor explicit type bindings before solving remaining variables. Removed the obsolete wire-only checked-inference wrapper.
- Added exact parameter realization using the same generic slot ordering as the entry frame. Named input conversion and final membership checks now use that contract before receiver execution. Definition-carrying host types are materialized before inference/conversion and retained with the frame.
- Added `named_generic_calls_preserve_live_inference_and_reject_conflicts_before_execution`. Focused execution and affected generic/compiler checks are pending. See F18; do not count the named-input gate complete yet.
- The focused test passes **1/1** (5.17s build, 2.91s execution). Its first attempt failed because the fixture reused an existing `record` function name; the new helper is now `record_pair`. Shared-solver compile checks exposed and fixed generic trait/borrow annotations before execution. The broader engine/generic selection is running; no full-suite result is claimed yet.
- The initial broader selection completed **249/252 passed**, two existing skips. Failures exposed missing function/actual-type context in input errors, incorrect plain-null storage for an inferred opaque host type, and a callback test assuming a fixed union-arm index. Corrected the first two; their focused tests pass. The union test now validates the selected index against the transmitted schema, rather than preserving an obsolete arm order.
- Extended the new probe with `Source<Output=O>`: infer a runtime-created associated output from a returned BAML implementation and call it through a named generic function. This passes after supplying the factory selector explicitly to the positional engine API. Final focused union and combined checks are pending.
- Final focused union check: **1/1 passed**, 5.51s build plus 2.79s execution. `cargo nextest run -p baml_type -p baml_type_runtime --lib -j4`: **256/256 passed**, 9.98s build plus 0.41s execution.
- `cargo nextest run -p bex_engine --lib --test generics_inference --test generics_explicit --test host_value_callable --test interface_views --test interface_handles --test fn_typed_slots --test call_layouts -j4 --no-fail-fast`: **252/252 passed**, two existing skips (56.46s build, 122.99s execution). Includes the associated-output extension and all final production fixes. Bridge all-target compilation and final formatting checks are in progress.
- `cargo check -p bex_engine -p bridge_ctypes -p bridge_cffi --all-targets -j4`: **passed**, 2m13s. Formatting, `git diff --check`, and internal-document whitespace/fence checks passed. No snapshots changed in this continuation.
- Next: carry these checked operations through the common native reference/method protocol, including exact type-reference evidence for specialization and sparse annotations. The current interface binder still accepts method type arguments by wire name, and the CFFI callable-handle entry rejects supplied type arguments. Complete those paths with the reference protocol rather than recreating runtime-created declarations by name. Then execute the generated Python vertical slice and portable SDK fixtures. Dynamic aliases/annotations, native registration, transactional ownership/scopes and all backend ports remain open. No BEP, website, PR, push or CI update.

### Shared native invocation continuation — F19

- Consolidated C, Python, Node, Java and Wasm request preparation/invocation. A prepared call owns temporary target/type pins before scheduling. Added a checked interface-method target; keyword binding uses the interface signature, including required names and optional modes.
- Unified engine type bindings as static names, explicit portable definitions or retained reflected-type references. Session type outputs retain live identity. Removed silent foreign-live-type import from ordinary value conversion as well as specialization.
- Added encoded-request integration probes for method defaults, renamed/reordered implementation arguments, unchanged receiver state on rejection, interface/type/callable disposal after preparation, named generic live evidence, wrong-role/closed/ambiguous references and foreign-runtime rejection. Verification is in progress. An initial test build exposed a mistaken `.unwrap()` in the new test helper; it is corrected. Native adapter checking exposed Java's remaining async dispatcher and Node's exhaustive error conversion; both are updated, with the rerun pending.
- Keep checkpoint 3 open: these changes do not replace the old handle table or solve ordinary argument partial-adoption cleanup, final schema negotiation, scopes or generated SDK support. Regeneration of all schema consumers and native execution remain required before the one-PR cutover. No BEP or website edit, PR, push or CI run.
- Encoded checks pass **3/3** after fixing fixture syntax: defaults require an interface method body, and signature-only methods require explicit effects. The new associated-output case compiles the same Named declaration twice in one runtime, rejects mismatched type/value evidence before mutation, then invokes a valid retained result after releasing its original references. Added an abandoned-prepared-call weak-reference assertion afterward; its final rerun is pending.
- `cargo check -p bridge_cffi -p bridge_python -p bridge_typescript -p bridge_java -p baml_bridge --all-targets -j4`: **passed**, 2m56s. Regenerated Rust/Python proto clients through the normal build and Node's protobufjs clients through `pnpm --dir sdks/typescript/bridge_typescript build:proto` (**passed**). The remaining schema consumers and generated interface APIs are still open.
- Removed the now-unused duplicate named/callable encoders. Updated the old engine unit test to require explicit portable import after foreign-reference rejection. Its first compile needed a qualified EngineError name; corrected before the final run. Affected engine tests and Wasm compilation are running.
- `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features -j4`: **passed**, 2m57s. `cargo nextest run -p bridge_ctypes -p bridge_cffi --lib --test interface_calls -j4 --no-fail-fast`: **105/105 passed**, 39.91s build plus 8.21s execution. Includes all three encoded probes and the final weak-reference assertion proving an abandoned prepared call releases its view while the runtime remains open.
- Final engine selection: `cargo nextest run -p bex_engine --lib --test generics_inference --test generics_explicit --test host_value_callable --test interface_views --test interface_handles --test fn_typed_slots --test call_layouts -j4 --no-fail-fast`: **252/252 passed**, two existing skips (2m08s build including artifact-lock wait, 129.21s execution). Includes foreign-live-type rejection plus explicit import, and all final production changes in this continuation.
- Next: finish the common transactional reference protocol and schema negotiation, including partial argument/result failure and sparse exact-type annotations, before the generated Python vertical slice and other SDK ports. A successful prepared call and shared Rust adapter checks are not proof of native generated APIs, host registration, scope drain or all-language parity. No snapshots, BEP, website, PR, push or CI changes in this continuation.

### Aggregate transfer ownership — F20

- Inbound decoding now adopts all handle occurrences from the parsed aggregate before interpreting annotations or values. A batch owns converted engine references and host registration Arcs; failure releases visited and unvisited siblings. Shared call preparation captures kwargs before call-id, target and type validation, so those early failures also clean up transferred arguments.
- Outbound value encoding records every table insertion under one rollback guard. Callback required/optional arguments share that guard; a later encoding failure no longer leaves earlier handles committed. This covers graph encoding, not final transport delivery/adoption.
- Added checked host-value interning: a conflicting host kind returns a boundary error without a debug panic or changing the established registration. Host-value input annotations now follow the same validation path as other values.
- Added repeated nested-failure tests with strong-reference/table counts, caller-original preservation, whole-batch keyword/annotation rejection, decoded ownership, wrong-kind identity preservation and early encoded-call failure. `cargo check -p bridge_cffi -j4` passed (57.30s); codec tests and strengthened CFFI integration checks are pending.
- Keep the full protocol gate open. Malformed bytes before envelope parsing, transport rejection/cancellation, host-side partial output adoption, outbound raw host-key lifetime and the final role/session/schema cutover still need the common transfer protocol. No BEP or website edits.
- Codec checks passed **68/68** (1m40s build, 0.11s execution). The first combined shared run passed **118/118** with one nextest output-handle leak report on the existing `host_spans::test_enter_exit_depth`; that test only exercises an in-memory stack. The final combined run below passes without the report.
- Added native safepoint draining for rejected preparation, plus whole-payload adoption before callback completion-flag validation. Added actual release-callback assertions for both early-failure paths; consolidated success/throw payload decoding.
- Final shared command: `cargo nextest run -p bridge_ctypes -p bridge_cffi -p bex_resource_types --lib --test interface_calls -j4 --no-fail-fast`: **120/120 passed**, no skips or output-handle leak reports. The preceding `cargo nextest run -p bex_engine --test host_value_callable --test interface_views --test interface_handles -j4 --no-fail-fast` passed **58/58**, one existing skip (2m01s build including lock wait, 51.86s execution). That engine run predates only the final shared preparation-error drain/completion-flag changes, which the 120-test run includes.
- Next protocol work is specified in design §8.1: transport-owned receipts outside value bytes, staged engine/host ownership, provisional SDK decoding, atomic claimed-lease adoption and late-result discard. Replace the callback error flag with an explicit completion envelope that can carry a present null throw. These are required before the new ABI can ship; adding rollback to the old codec alone does not close checkpoint 3.
- Final shared run timing: 21.84s build plus 5.60s execution. Native check `cargo check -p bridge_cffi -p bridge_python -p bridge_typescript -p bridge_java -p baml_bridge --all-targets -j4` passed in 1m17s; Wasm check `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features -j4` passed in 2m08s including lock wait. Moved a test-only import after the native check's unused-import warning; a final CFFI all-target check is running. No snapshot changes, BEP/site edits, PR, push or CI run.
- Final `cargo check -p bridge_cffi --all-targets -j4` passed in 1m12s without warnings after the import cleanup. Formatting, `git diff --check`, and internal-document whitespace/fence checks passed. The next gate remains the versioned transport receipt/adoption protocol; the implementation goal is not complete.

### Staged outbound delivery and receipts — F21

- Added shared `EncodedTransfer`, `TransferSession`, `TransferReceipt` and `PendingDelivery` ownership. Encoding owns the complete aggregate, staging assigns a session-qualified receipt outside the bytes, sender abandonment discards, and successful transport handoff leaves explicit adoption/discard responsibility with the receiver. Completed entries are removed, including for a session that stays open indefinitely.
- Adoption validates claimed lease multiplicities before removing a pending transfer. It transfers only claimed engine leases, releases unclaimed cache-hit/skipped values, and releases retained host registration Arcs after decoding has had a chance to rehydrate them. Wrong-session, unknown receipt and excessive/foreign claims cannot consume pending ownership. Session close serializes with staging/adoption and discards pending outputs; it does not claim to cancel/drain running work.
- `OutboundEncoder` shares aggregate ownership but isolates failed individual encode attempts. A fallback SdkPanic cannot accidentally retain an earlier, partially encoded return value. Callback required/optional arguments use the same owned encoder. Shared result classification now accepts this encoder; `invoke_prepared_encoded` exposes the owned result through the actual named/callable/interface dispatcher.
- Fixed last-handle release dropping arbitrary RustData/ADT state under the table write lock. Destruction now occurs after unlocking, as does receipt cleanup after the session lock. Tests assert destructors can reacquire the relevant locks.
- `cargo nextest run -p bridge_ctypes --lib -j4 --no-fail-fast`: **79/79 passed** before adding the final table-lock/CFFI tests. The first combined run passed **126/126**. Final `cargo nextest run -p bridge_ctypes -p bridge_cffi --lib --test interface_calls -j4 --no-fail-fast`: **128/128 passed**, 42.72s build plus 8.119s execution. Includes repeated no-history adoption, duplicate leases, unknown/wrong-session claims, host retention, encoding panic/failed fallback, callback aggregates, close/stage and adopt/discard races, final session drop, and real interface invocation after adoption. The two final checks prove a panicking resource destructor cannot orphan sibling leases or leave provisional SDK claims owned by nobody.
- Final `cargo check -p bridge_cffi -p bridge_python -p bridge_typescript -p bridge_java -p baml_bridge --all-targets -j4`: **passed**, 1m08s. Final `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features -j4`: **passed**, 1m22s including lock waits. `cargo fmt --all --check`, `git diff --check`, and internal-doc fence/whitespace/local-file-link checks pass. All launched processes are terminal; no test or CI job remains running from F21.
- Keep checkpoint 3 open. Existing native entrypoints still call the explicitly named `into_unreceipted` transition while their return types and decoders are ported. This is temporary implementation scaffolding to remove at the final ABI cutover, not a compatibility promise or a completed transport solution. No generated SDK is claimed to implement receipts yet.
- Next: connect receipt sessions to issuing runtime/transport lifecycle; return an owned native result/envelope that retains cleanup through failed future conversion/delivery; root provisional SDK wrappers and rehydrate host errors before adopting their exact claims; discard on parse/decoder failure and cancellation; drain queued host releases at safe native boundaries. Port callback arguments, unhandled errors and other capability-returning APIs too. Finish role/session/schema negotiation and explicit callback completion arms (including present null throws). Keep BEP/site unchanged; no PR, push, CI or snapshot changes in this continuation.

### Python owned result delivery — F22

- Native/Wasm singleton slots now store the engine and its `TransferSession` together. `get_runtime_with_transfers` takes one consistent snapshot. Replacement and shutdown close the previous session outside the global slot lock, before engine shutdown; old calls cannot stage output into the replacement session. Added a shared encoded-interface lifecycle case for replacement and shutdown.
- Python function calls, synchronously and through PyO3 futures, return `BamlEncodedResult` instead of raw bytes. The object owns its `PendingDelivery` until decoding adopts or discards it; destruction handles unobserved results and cancelled/failed future delivery. Pre-call errors use the same owned envelope through shared error classification.
- The Python decoder uses a nested ContextVar transaction. Handles are provisional and strongly rooted through Pydantic validation. An escaped provisional handle cannot be called/copied/encoded before adoption or after failure. Successful decode activates its leases; invalid/unused children are released. A decoded BAML error is adopted before raising, so its live children and rehydrated host exception survive. Absent result outcomes are rejected rather than treated as null success.
- Adopted handle state retains the issuing runtime. Failed decoding clears that runtime ownership even when a validator retained an invalid handle. Receipt cleanup drains queued host releases at the native safepoint. Local data/media constructors keep their ordinary independent ownership.
- Added real Python/native tests for unread results, successful callable adoption, partial/parse failure, Pydantic rejection and replacement, reentrant validation, declared errors with callable fields, runtime replacement, async results and unobserved completed futures. Verification is pending. The initial native build passed in 2m54s; its test collection found it predated the newly added transfer-count helper. Rebuild and stub generation are running, with Cargo artifact-lock contention. No failed test body is counted as passed.
- `cargo check -p bridge_python -p bridge_cffi --all-targets -j4` passed in 1m25s before the final helper/signature edits. The shared ctypes/CFFI selection passed 128/128 before adding the singleton replacement/shutdown test. Final checks remain pending.
- Keep the full protocol/SDK gate open: Python callback arguments, unhandled errors and introspection results still need owned delivery; the remaining languages still need native receipt envelopes and provisional decoders. Runtime-selected generated refs, host-interface registration, scopes, all-language fixture execution, and final ABI/schema negotiation are not completed by this result path. BEP and website remain unchanged.

#### F22 verification follow-up

- Resumed implementation after the A10 documentation review. The initial 12 native result tests passed; extending verification exposed six failures in existing Python tests. Builtin runtime failures depended on absent generated typemap entries, masking their real diagnostics and host exception identity. Added SDK-owned builtin failure decoding and exact class pass-back; application errors retain generated model validation. Corrected missing-function classification to InvalidArgument and tests to expect HostContractViolation for invalid host results and original identity for permitted host exceptions.
- Final native run: `uv run --project sdks/python --no-sync pytest sdks/python/tests/test_result_transfers.py sdks/python/tests/test_host_callable.py sdks/python/tests/test_engine.py sdks/python/tests/test_decode_handle.py sdks/python/tests/test_media.py sdks/python/tests/test_portable_values.py -n0 -q` — **88 passed, 2 xfailed, 1 xpassed in 172.32s**. Includes all 17 new result-delivery cases: unread/cancelled results, model rejection/replacement, reentrant validation, live children in declared errors, original host exceptions, null error presence, builtin failure pass-back and decoding with a deliberately broken app typemap. Existing xfails are not accepted lifetime evidence.
- The expanded shared run found the checked-in C header missing the existing Rust Interface=19 discriminator. `cargo nextest run -p bridge_cffi --test header_generation -E 'test(=regenerate)' --run-ignored only -j4` regenerated that single enum addition. Then `cargo nextest run -p bridge_ctypes -p bridge_cffi -j4 --no-fail-fast` — **135/135 passed**, one ignored header-regeneration test (26.09s build, 9.733s execution). No snapshot updates.
- Latest native extension build: `uv run --no-sync maturin develop --uv` from `sdks/python` — **passed in 19.88s**. Stub generation completed earlier through `cargo run -p bridge_python --bin stub_gen -j4` under the Python uv environment; the result payload getter is explicitly bytes. Ruff passes on the changed decoder/error/test modules, and `git diff --check` passes. Final checks: `cargo check -p bridge_cffi -p bridge_python -p bridge_typescript -p bridge_java -p baml_bridge --all-targets -j4` passed in 1m25s; `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features -j4` passed in 9.43s. Cargo formatting and internal-document structure checks also pass. All processes launched in this continuation are terminal.
- Remaining ownership work includes general retained opaque host-reference adoption, not only same-host exception recovery before receipt release. The current raw host-key paths and remaining unreceipted output paths must switch with the unified ABI. Do not claim complete bridge lifetimes, quiescent callback collection, scopes or generated interface APIs from this Python function-result slice. No BEP/site edits, PR, push or CI run.

### Owned host references — F23

- Added `CffiHandleTableEntry::HostValue` and the HostReference discriminator. Returned host values now own table leases; removed the separate outbound host-Arc ledger. Raw host keys remain at initial registration only. Receipt adoption can retain selected host references after delivery and discard unused ones; ordinary clone/release and inbound capture preserve their original HostValueArc identity.
- Python same-host lookup resolves an owned table entry to its registration. Last owned handle release drains host notifications outside table locks. Python drops its retained runtime state before its final release drain, so a final runtime owner cannot leave a queued host release waiting for an unrelated later call. The C handle-release entrypoint catches resource-cleanup panics.
- Added shared tests for selected versus unclaimed host references, copy/pass-back identity for callable/opaque kinds, and last-reference cleanup through the actual C release boundary without a VM call. `cargo nextest run -p bridge_ctypes -p bridge_cffi --lib --test interface_calls -j4 --no-fail-fast`: **129/129 passed** (1m49s build, 9.148s execution).
- New native Python copy/pass-back, unread-result and escaped-invalid-wrapper probes: **3/3 passed** in 9.52s. Initial fixture probes failed because `$rust_type` is builtin-only and the protobuf map entry uses `string_key`; corrected the fixture to a legal generic identity function and the actual field. No language restriction was weakened. Added an additional partial input-encode rollback case; the broader native selection is running.
- `cargo check -p bridge_ctypes -p bridge_cffi -p bridge_python --all-targets -j4` passed in 1m56s before the final cleanup/test edits. Python native build passed in 2m05s. Rust/Python proto consumers regenerated through the normal build; Node protobuf generation passed. C header regeneration and final cross-adapter/Wasm checks are pending/running.
- Remaining ports: update each bridge's same-host lookup to resolve owned references, alongside its receipt decoder. In particular, Node `host_value_registry.ts`, Go's two host decoders, and C# `HostCallableProtocol` still recognize raw exception-registration keys. They must not reinterpret an owned table key as a raw registry key. Finish all schema consumers and version/fingerprint negotiation together before shipping. No generated interface SDK or complete cross-language lifetime support is claimed; BEP/site unchanged.

#### F23 verification follow-up

- Broader Python/native selection (the six files listed in F22, now including all 21 result-transfer tests): **92 passed, 2 xfailed, 1 xpassed in 179.22s**. All four new opaque-host cases pass, including partial input-encoding rollback. The original callback/media/result tests also pass with owned HostReference values.
- Regenerated the C header using the existing ignored nextest regeneration case. `cargo nextest run -p bridge_cffi --test header_generation --test abi_layout -j4`: **4/4 passed**, one ignored regeneration case. The header adds HostReference=20 alongside the prior interface discriminator; C/Rust/C++ layout checks agree. This does not establish the still-open final ABI negotiation.
- Ruff, Cargo formatting, diff whitespace and internal-document checks pass. Final native adapter compilation is running, followed by Wasm compilation. Remaining host-lookup ports are listed above; no cross-language completion claim or BEP/site change.

### Packed-ref build watcher — F24

- Found and fixed baml_artifact's nonexistent loose-ref watch for this packed branch. It now watches an existing ancestor plus packed-refs, retaining the same exact HEAD fingerprint. This removes the repeated compiler rebuild seen during otherwise narrow checks.
- Compiled and ran the real build script against a temporary Git repository through loose, packed, and packed-to-loose update states. All emitted watch paths exist; the fingerprint equals actual HEAD and changes for the new commit. The real workspace's consecutive `cargo check -p baml_artifact -j4` runs passed: first 39.47s, second **0.12s with no compilation**.
- The new C release panic test initially referenced a private helper type; corrected it to construct through the public BexExternalValue conversion. `cargo nextest run -p bridge_cffi --test interface_calls -E 'test(=resource_cleanup_panic_stays_inside_handle_release_abi)' -j4`: **1/1 passed**, seven unrelated tests filtered (1m39s build before the watcher fix, 0.019s execution). Native all-target checks are rerunning after that test correction and the watcher change, followed by Wasm.

F23/F24 final native compilation: `cargo check -p bridge_cffi -p bridge_python -p bridge_typescript -p bridge_java -p baml_bridge --all-targets -j4` **passed in 2m54s** after the test constructor correction and packed-ref watcher fix. Wasm compilation also passed: `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features -j4`, 2m07s. No additional behavioral changes were made after the passing Python/shared/C-ABI checks. All processes launched in this continuation are terminal. The implementation goal remains active; remaining transport, codegen and end-to-end gates above are not complete.


### F25. Node owned function-result delivery (function path verified; broader port open)

The prior goal pass made internal documentation progress (A11); this continuation resumes production implementation. Node sync/async function calls now stage owned results. Native handle owners are rooted through decoding and shared by media wrappers; adoption makes them usable, while discard invalidates escaped wrappers. Returned host references resolve through owned table entries rather than treating table keys as raw host-registration keys. The JS decoder adopts valid errors before throwing and propagates failure-model decoding errors instead of hiding them.

Added native-addon tests for explicit discard, provisional access, sync/async closure results, retained values after constructor failure, valid thrown references, reentrant decode, null throws and unread-result GC with the singleton still open. The final native-addon runs pass; see results below. Initial Rust check found two integration naming mistakes (table `resolve` and the module path for owned boundary errors); corrected before the addon build. Build and native execution results will be recorded below. Callback/introspection/unhandled-result delivery, broader SDK codegen, reference negotiation and other checkpoints remain open.


F25 verification:

- Native addon built with the N-API CLI invoked from the workspace root, explicit SDK manifest/package JSON and dist output: **passed, 3m18s** (`/tmp/baml-node-owned-build.log`). Generated native declarations, TS output and protocol artifacts were refreshed through the package's copy/build/tag steps. Final `cargo check -p bridge_typescript --all-targets -j4`: **passed, 10.38s**, with the two unused imports removed (`/tmp/baml-node-owned-check-final.log`). `build:ts_build` passed.
- First focused run exposed a test FQN mistake (`root.Bundle` versus the actual `user.Bundle`) and a skipped package tag step that fixes protobuf ESM imports. Corrected the test and ran the normal tag step. The reentrant test now asserts its constructor executed, preventing a false positive from an unmapped type.
- `pnpm --dir sdks/typescript/bridge_typescript exec vitest run tests/result_transfers.test.ts tests/test_decode_handle.test.ts tests/test_decode_call_result.test.ts tests/test_media.test.ts --maxWorkers=1`: **39/39 passed, 8.02s** (`/tmp/baml-node-owned-tests-final.log`).
- After adding owned-host-reference clone/pass-back and strengthening the GC test, `... vitest run tests/result_transfers.test.ts --maxWorkers=1`: **12/12 passed, 8.32s** (`/tmp/baml-node-owned-transfers-final.log`). The forced-GC subprocess keeps the singleton open, checks unread receipt removal, collects a provisional JS wrapper before adoption, then verifies its unused table lease was released after adoption.
- `... vitest run tests/host_callable.test.ts tests/test_engine.test.ts tests/call_function.test.ts --maxWorkers=1`: **83/83 passed, 94.22s** (`/tmp/baml-node-owned-integration.log`). Includes synchronous throws and rejected promises preserving original JS error identity. Together with the final 12 ownership cases and 28 decoder/media/handle cases, this is **123 distinct passing tests**.
- Formatting and scoped `git diff --check` pass. All commands launched for this continuation are terminal. No BEP/site edits, snapshots, PR/push or CI run. This is native bridge evidence, not generated interface SDK parity. Next work remains callback/introspection/unhandled result receipts, other bridge consumers, and the broader checkpoints above.

### F26. Owned callback arguments and Node queue cleanup (native paths verified; lifetime work remains)

Shared `sys_native` now encodes callback arguments as one owned transfer, with a native owned dispatch registration alongside the still-unported C byte transport. A missing dispatcher drops argument leases. Node and Python use the owned path; their decoders adopt the full argument list before invoking user code and discard it when an argument model rejects. Native messages keep their own delivery session, rather than looking up a potentially replaced global runtime during dispatch.

The Node queue required a transport fix: napi 3.9.0 loses boxed queue data on failed enqueueing and null-environment delivery. Added a bridge-owned queue that reclaims those messages and an abort guard for conversion/broken-handler failures. Test probes measure actual native resource ownership with Weak references, including full queues and the production null-environment delivery path. The separate release-notification queue still needs lossless delivery; see adjustment F26.

Initial Node addon build and TypeScript compilation passed. The first ownership/queue run passed 17/17; existing callback/function/media/envelope integration passed 107/107 in 93.79s. Python all-target check passed in 2m35s and its editable native build passed in 2m08s. Expanded retained-media callback tests and final queue checks also pass; see the results below. Scope/ABI/codegen/shared SDK parity requirements remain open, as do actual multi-environment termination and tracked-work lifetime tests.


F26 final verification:

- Node addon rebuilt after the final cleanup-panic guards: **passed, 31.64s** (`/tmp/baml-owned-callback-node-build-final.log`). Native declarations and TS output refreshed with `build:copy-native-dts`, `build:ts_build`, and `build:tag-generated-files`; all passed.
- `pnpm --dir sdks/typescript/bridge_typescript exec vitest run tests/dispatch_queue.test.ts tests/result_transfers.test.ts --maxWorkers=1`: **22/22 passed, 9.58s** (`/tmp/baml-owned-callback-node-tests-final.log`). Includes all four retained-media callbacks; callback argument adoption and failure before user entry; queue-full and null-environment cleanup; handler failure before/after adoption; earlier F25 ownership cases.
- The existing five-file Node integration selection (`host_callable`, `test_engine`, `call_function`, `test_media`, `test_decode_call_result`) passed **107/107, 93.79s** (`/tmp/baml-owned-callback-node-integration.log`). Total distinct Node cases this pass: **129**. That integration run preceded only the final cleanup-panic guards and added tests; the final 22-case run uses the rebuilt addon.
- Python's focused callback selection passed **6/6, 22.67s**. Final `uv run --project sdks/python --no-sync pytest sdks/python/tests/test_result_transfers.py sdks/python/tests/test_host_callable.py sdks/python/tests/test_engine.py sdks/python/tests/test_decode_handle.py sdks/python/tests/test_media.py sdks/python/tests/test_portable_values.py -n0 -q`: **98 passed, 2 xfailed, 1 xpassed, 218.80s** (`/tmp/baml-owned-callback-python-final.log`). The expected/unexpected markers are existing tests, not acceptance waivers for this port. The new Python callback test checks adoption by cloning a lease, then invokes the retained closure after the callback returns; it does not claim synchronous BAML reentry on the Tokio callback worker is supported.
- `cargo nextest run -p sys_native -E 'test(/host_dispatch::|host_impls::/)' -j4 --no-fail-fast`: **14/14 passed, two skipped**, 3m24s compilation and 0.067s execution (`/tmp/baml-owned-callback-shared.log`). New tests prove missing/rejected/panicking native dispatch drops the aggregate. Warnings are in the untouched feature-gated `registry.rs`.
- Scoped formatting, Ruff and `git diff --check` pass. All commands started in F26 are terminal. No BEP/site edits, snapshots, PR/push or CI run. The lossless release queue, C transport port, complete runtime/session negotiation, host-interface registration, scope/work draining, generated interfaces and all-language SDK execution remain required.

### F27. Acknowledged Node release batches (transport verified; broader lifetime gates open)

Replaced the bounded per-key host-release queue with a native pending set and one coalesced wakeup. JS acknowledges a batch by finishing its idempotent registry deletions. Reentrant releases join pending work; partial delivery failure retries; empty pending sets release their allocation. The channel is unreferenced and clears its native storage during environment finalization. Unexpected native enqueue errors retain keys for a later enqueue retry rather than claiming successful delivery. Callable-owner removal continues outside the registry lock. See adjustment F27 for the process-global registry limitation that remains.

Verification:

- `cargo check -p bridge_typescript --all-targets -j4`: **passed, 19.99s** (`/tmp/baml-release-queue-check.log`). The lib-test build reports dead-code warnings for the two native probe exports, which the JS tests invoke through N-API.
- Rebuilt the native addon with the N-API CLI from the workspace root and explicit SDK manifest/package/output paths: **passed, 11.11s** (`/tmp/baml-release-queue-build.log`). `build:copy-native-dts`, `build:ts_build`, and `build:tag-generated-files` passed; generated declarations and JS reflect the batch callback signature.
- `pnpm --dir sdks/typescript/bridge_typescript exec vitest run tests/release_queue.test.ts tests/dispatch_queue.test.ts tests/result_transfers.test.ts --maxWorkers=1`: **29/29 passed, 10.78s** (`/tmp/baml-release-queue-tests.log`). This run preceded the additional full-width-key test; the final run below includes it. An initial `pnpm test` command rejected the CLI option before execution; switched to explicit `pnpm exec vitest`.
- Existing callable/function selection: **61/61 passed, 67.61s** (`/tmp/baml-release-queue-integration.log`). Three additional file filters used incorrect names and selected nothing; their actual `test_`-prefixed files are all included in the final run below.
- Final `pnpm --dir sdks/typescript/bridge_typescript exec vitest run tests/release_queue.test.ts tests/test_engine.test.ts tests/test_media.test.ts tests/test_decode_call_result.test.ts --maxWorkers=1`: **54/54 passed, 18.81s** (`/tmp/baml-release-queue-final-tests.log`). Together these runs cover **137 distinct passing Node cases**. The eight release tests cover a 16,384-registration production burst, repeated 5,000-registration bursts with idle capacity zero, reentrant delivery, partial-failure retry, duplicates, all 64 key bits, close-before-delivery, and subprocess exit without explicit runtime shutdown.
- Cargo formatting and scoped diff-whitespace checks pass. These tests verify removal from the real JS registry and release-queue storage; they do not assert deterministic host garbage collection. No Python/shared behavior changed in F27, so their passing F26 suites were not repeated.

The internal design now states that release notification must remain owned until acknowledgment. Quiescent VM collection, cross-runtime cycles, scope/work draining, worker-specific routing, remaining bridge transports, final ABI negotiation and generated SDK execution remain open. No BEP/site edits, snapshots, PR/push or CI run. The full implementation goal remains active.

### F28. Owned unhandled spawned-error delivery (native paths verified; protocol work remains)

The preceding F27 turn made verified implementation progress. Shared unhandled-error encoding and its pending callback registry now retain an EncodedTransfer instead of committing table leases into bare bytes. Python and Node register native owned handlers, stage delivery receipts, and use their normal transactional result decoders. Node's existing owned N-API queue now carries either a host invocation or an error notification, with the correct arguments/failure action for each. Notifications use an unreferenced, unbounded channel and nonblocking enqueueing; undelivered or rejected messages retain cleanup ownership. A broken reporter discards only provisional leases, preserving already adopted refs.

The new native tests use actual spawned errors carrying opaque host objects, not a mocked registry or encoder. They cover unread envelopes after runtime shutdown, adoption before raising, rejection with escaped children, and reporter failure before/after adoption. A strict Python test exposed a missed release drain after the engine dropped its reporting copy. Added a safepoint drain after unhandled reporting and a final drain after runtime drop during bridge shutdown. The new engine case exercises collection while the engine stays open; the Python test checks weak-reference reclamation before any later SDK operation. The adjustment log separates this native issue from an initial test-local traceback reference.

Initial verification: native all-target check passed (32.05s); first Node addon build passed (21.47s), Python editable build passed (40.88s). Seven focused shared encoding/notification tests passed after 2m16s compilation. The first Node run had one over-specific assertion that discarded bytes must become unreadable; corrected it to assert adoption rejection and host-registry removal. The resulting five-file Node selection passed 35/35 (37.67s). Python initially passed the three basic ownership cases (12.19s); the expanded selection passed 31/32 and exposed the reporter cleanup issue described above. These initial results precede the final safepoint drains.

Both addons were rebuilt after the drain fix: Python passed in 1m09s, Node in 1m18s (including lock waits). Package native-declaration copy, TypeScript compilation and generated-file tagging passed. Final Python/Node selections, engine fire-and-forget tests, and native/Wasm compilation are recorded below. Keep the broader checkpoints unchecked. The remaining C byte route, runtime/environment routing and pending-notification teardown policy still need the coordinated final protocol; this is not a compatibility guarantee. Collector codecs are still an explicit port/cleanup item, and their current no-op producer provides no live integration coverage. No BEP/site changes.

F28 final verification:

- `uv run --project sdks/python --no-sync pytest sdks/python/tests/test_owned_spawn.py sdks/python/tests/test_result_transfers.py -n0 -q`: **32/32 passed, 127.97s** after rebuilding with both release drains (`/tmp/baml-owned-spawn-python-verified.log`). The additional subprocess test for the unchanged default fatal-reporting policy passed **1/1, 4.59s** (`/tmp/baml-owned-spawn-python-default.log`). Total distinct Python cases: **33**. The reporter test checks reclamation before invoking any later SDK method.
- `pnpm --dir sdks/typescript/bridge_typescript exec vitest run tests/owned_spawn.test.ts tests/unhandled_spawn.test.ts tests/dispatch_queue.test.ts tests/result_transfers.test.ts tests/release_queue.test.ts --maxWorkers=1`: **35/35 passed, 39.59s** on the rebuilt addon (`/tmp/baml-owned-spawn-node-verified.log`). Includes the existing default fatal-reporting subprocess, all F27 release cases, and retained callback media.
- `cargo nextest run -p bridge_cffi --lib -E 'test(/unhandled_spawn::|baml_to_host::/)' -j4 --no-fail-fast`: **7/7 passed**, 2m16s compilation plus 0.051s execution; 36 unrelated tests filtered (`/tmp/baml-owned-spawn-shared-tests.log`). This precedes only the final engine/shutdown drains, which the native and engine tests exercise.
- `cargo nextest run -p bex_engine --test fire_and_forget -j4 --no-fail-fast`: the **25 existing cases passed**; the new case initially assumed reclamation in the reporting collection itself. Inspection confirmed the VM object remains rooted for materialization until that collection finishes. The corrected test asserts reporting followed by reclamation in a subsequent collection while the engine stays open. `... -E 'test(=reported_host_value_is_reclaimed_by_subsequent_collection)'` then **passed 1/1**, 10.31s compilation plus 5.04s execution (`/tmp/baml-owned-spawn-engine-final.log`). All 26 distinct engine cases have passing results; the original complete run's failed test and correction remain visible in `/tmp/baml-owned-spawn-engine-tests.log`. Automatic scheduling of those collections remains an open gate.
- Final `cargo check -p bridge_python -p bridge_typescript --all-targets -j4`: **passed, 2m00s including lock wait** (`/tmp/baml-owned-spawn-check-final.log`), with the two existing lib-test dead-code warnings for native probe exports. Final `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features -j4`: **passed, 49.00s** (`/tmp/baml-owned-spawn-wasm-final.log`). Both native addon builds and package generation steps are recorded above; no SDK schema/stub change was required in Python because callback registration already accepts a Python callable.
- Ruff passes for the new Python tests and modified decoder. A broader lint check of `__init__.py` reports two existing unused-reexport warnings for `get_bridge_runtime_version` and `get_toolchain_version`; this turn changes only the unhandled reporter's argument annotation there. Scoped diff-whitespace and formatting checks pass. No BEP/site edits, snapshots, PR/push or CI run.

This closes the Python/Node unhandled-error delivery port, not checkpoint 3 or the full goal. C transport receipts, final reference/session/environment routing and negotiation, pending-notification teardown, quiescent collection, generated interface APIs, all-bridge SDK execution, providers/Agent/media and alias cutover remain required.

### F29. Python owned interface refs and native method dispatch

The preceding A12 turn made documentation/source-audit progress; this turn implements its Python runtime prerequisite. `proto._decode_handle` now creates an owned interface wrapper, optionally resolving a facade subclass from `BamlTypeMap.interface_refs`. The new `_interface.py` owns local close/copy behavior and dispatches checked method calls. `_call_method` prepares the receiver and transferred arguments synchronously, then invokes the runtime retained by the adopted handle. The reference also retains that issuer's transfer session and its SDK typemap. Methods therefore neither select a later singleton nor decode nested results with an unrelated later typemap. Error decoding adopts nested references before raising.

`TransferSession::is_closed` rejects calls begun after issuer close/replacement. This is an admission observation, not a complete teardown barrier: a race after admission still relies on session staging rejection, and full cancellation/drain semantics remain required. Closing one Python ref leaves copies and already prepared calls independently owned. Missing-event-loop failure drops prepared arguments before a native guard drains their host releases. The generated native stub describes the async result as `Awaitable[BamlEncodedResult]`.

Added `sdks/python/tests/test_interface_refs.py`, using the actual shared `sdk_tests/fixtures/interfaces` BAML source plus focused local declarations. It tests default methods and pass-back; shared mutable owner state and independent close/copy; prepared-call retention; rejection before receiver mutation; exact explicit generic specialization; Error-only associated-pin mismatch; non-class array/string receivers and nullable iterator items; captured typemap decoding; adopted interface errors; shutdown/replacement rejection; failed decoder/encoder cleanup; and scheduling failure with a real weakly observed host callback. Small handwritten facade subclasses exercise decoder registration; they are deliberately not counted as generated SDK coverage or shared native parity execution.

Initial verification: `cargo check -p bridge_python --all-targets -j4` passed; the editable addon built in 40.35s. The first 13-case run passed 12 and exposed a test helper's `name` keyword collision; making helper control arguments positional-only fixed the helper. Adding the Error-only case produced a combined **47/47** run with the existing result-transfer and owned-spawn suites (**47.00s**, `/tmp/baml-python-interface-verified.log`). After the future-conversion cleanup guard and its new test, the focused interface suite passed **15/15, 19.88s** (`/tmp/baml-python-interface-final.log`).

Native stub generation passed (initial dependency rebuild 3m22s; precise signature regeneration 51.08s), and the emitted stub was copied to `src/baml_bridge/baml_py.pyi`. The final editable addon rebuild passed in **24.59s** (`/tmp/baml-python-interface-addon-verified.log`). Formatting, Ruff and scoped diff-whitespace checks pass.

Final Python verification: from `sdks/python`, `uv run --no-sync pytest tests/test_interface_refs.py tests/test_result_transfers.py tests/test_owned_spawn.py tests/test_typemap.py tests/test_proto_decode_typemap.py -q` passes **60/60, 55.62s** on the final rebuilt addon (`/tmp/baml-python-interface-all-verified.log`). This includes all 15 interface cases and the scheduling-failure cleanup test.

Shared verification: from the workspace root, `cargo nextest run -p bridge_ctypes --lib -E 'test(transfer::tests::)' -j4 --no-fail-fast` passes **13/13**, 68 unrelated cases filtered, after **2m59s** compilation and **0.034s** test execution (`/tmp/baml-python-interface-transfer-tests.log`). This covers adoption/discard and close/staging races, independent lease claims, wrong-session rejection and cleanup outside locks. All commands in this continuation are terminal; no snapshots, BEP/site changes, PR/push or CI run.

Remaining work: connect the shared codegen graph to emitted typed refs, input/host roles, dispatch methods and typemap registration; generate live concrete implementation facades; carry issuing runtime/session information through callback and unhandled-error delivery; implement native host registration and generic-adapter ergonomics; then execute actual generated SDK fixtures across all bridges. Generic `_invoke` currently requires the shared target's explicit method type arguments; inference and checked dynamic type-token APIs remain open. No promise of arbitrary cross-runtime invocation, automatic cycle collection or complete shutdown/drain follows from these tests. All broader plan gates, one-PR delivery and the BEP/site hold remain intact.

### F30. Generated Python interface callers (partial vertical slice verified)

The previous F29 turn made verified implementation progress. This turn connects the shared graph to generated Python ref classes, signatures, method dispatch and typemap registration. The graph now exports compiler-derived existential/concrete-Self/receiverless callability instead of asking each backend to re-derive the rule. Associated parameters retain declaration order and exact Error pins; Python `never` now has its uninhabited `typing.NoReturn` projection. Methods with explicit type parameters currently require `_types`; inference ergonomics and native host implementation roles remain open.

The first generator test attempt exposed 84 stale test constructors from the earlier SymbolPool-map-to-struct migration. Changed the test fixtures to construct SymbolPool directly; the subsequent full Python generator suite passed 124/124. The current CLI generated the shared interface fixture into `/tmp/baml-python-generated-interface-review/generated/baml_sdk`. Four of the five existing shared Python interface cases execute successfully there, using actual generated methods: receiver round-trip, defaults, owner-side state and copied records with live children. **The concrete-implementation input case still fails**: FriendlyGreeter is still decoded as a copied Pydantic data model, and its pass-back cannot supply the required live interface view. Keep that case enabled. Implementing concrete facades and the general input role is the next vertical-slice requirement, not a reason to weaken admission or add `.as_client()`.

Final verification:

- `cargo nextest run -p sdkgen_python_pydantic2 -p baml_ide -E 'package(sdkgen_python_pydantic2)|test(interface_export)' -j4 --no-fail-fast`: **132/132 passed**, 264 unrelated tests filtered; 5.55s compilation and 3.550s execution (`/tmp/baml-interface-f30-tests-final.log`). The three new generator tests check shadowed generic slots, qualified associated projections without Any erasure, and declaration-order bindings. The export test checks existential/default, concrete-Self and receiverless members; existing source-less export coverage passes. After simplifying the export closure, its five selected tests passed again in 3.490s (`/tmp/baml-interface-f30-export-final.log`).
- `cargo build -p baml_cli --bin baml-cli -j4`: **passed, 30.79s**. The rebuilt CLI generated **253 Python SDK files** from the shared BAML fixture in the temporary review project, with zero identifier renames (`/tmp/baml-interface-f30-cli.log`, `/tmp/baml-interface-f30-generate.log`). This output includes token/ref associated metadata and the method type-token helper. Subsequent Rust changes were tests and equivalent export cleanup, with no caller-output change.
- With `PYTHONPATH=/tmp/baml-python-generated-interface-review/generated`, `sdks/python/.venv/bin/python -m pytest sdk_tests/crates/python_pydantic2/interfaces/customizable/test_interfaces.py -n0 -o asyncio_mode=auto -q`: **4 passed, 1 failed, 0.40s**, exactly the enabled concrete-factory failure above (`/tmp/baml-interface-f30-native.log`). No xfail/skip or relaxed conformance check was added.
- Added `sdk_tests/crates/python_pydantic2/interfaces/customizable/interfaces_static.py` for the harness's Pyright step. Running Pyright against that consumer with the generated SDK on extraPaths gives **zero diagnostics** (`/tmp/baml-interface-f30-static.json`). The persistent negative probe `interface_probes/python/generated_interface_wrong_pins.py` gives **exactly two reportAssignmentType errors**, one for Output and one for Error alone (`/tmp/baml-interface-f30-negative.json`). These are consumer checks, not a claim that the complete generated package/harness type-checks.
- `PYTHONPATH=/tmp/baml-python-generated-interface-review/generated sdks/python/.venv/bin/python interface_probes/python/generated_interface_callers.py`: **passed** (`/tmp/baml-interface-f30-generated-probes.log`). This executes generated generic methods with native and definition-carrying type tokens, passes a live Counter through generic dispatch, compares token/ref associated pins, and calls generated array/string iterators including null-versus-Done. This supplemental probe is not shared parity coverage.
- `sdks/python/.venv/bin/python -m pytest sdks/python/tests/test_interface_refs.py sdks/python/tests/test_typemap.py sdks/python/tests/test_proto_decode_typemap.py -n0 -q`: **29/29 passed, 38.88s** (`/tmp/baml-interface-f30-runtime.log`). New cases verify exact method type-choice keys/order and definition preservation, plus distinct ordinary/associated token arguments. The existing 15 native interface cases still pass. No native Rust addon change was needed in F30.
- Scoped Cargo formatting, Ruff and diff-whitespace checks pass. An initial new-test compilation used nonexistent convenience constructors; the tests now build explicit types/context and pass. No snapshots, BEP/site changes, PR/push or CI run.

The Python checkpoint, all-interface support and shared parity remain incomplete. Generic method inference, required-interface method composition, general symbolic projections and concrete-Self APIs still need their full generated paths. Input/host roles, issuing-runtime callback propagation, the final protocol and other bridges remain required.

### Next: concrete receivers and input acceptance

1. **Metadata implemented and verified in F31.** Compute one compiler-owned class boundary projection consumed by both bytecode and SDK metadata. Distinguish authored instance behavior (including implementation defaults and out-of-body rules) from static factories, empty markers and compiler-generated serialization. Preserve dedicated builtin data codecs and copied outer records with live children; do not infer liveness transitively from fields.
2. Carry a retained concrete-object reference with exact receiver/class arguments through owned delivery. Share ownership/session machinery with interface refs. Its trusted reference role, rather than its display name or a stream wrapper convention, controls decoding.
3. Generate the public concrete facade and its authored methods/factories without copied live fields. Keep necessary record/config initialization as a distinct operation. At an interface input, validate/project this existing receiver automatically using the shared obligation checker.
4. Emit the native input role so a generated concrete facade type-checks directly. Check inherited requirements and associated pins; do not enumerate known implementations as the interface's closed set.
5. Make the currently failing FriendlyGreeter factory test pass and add owner-state/copy-close/error tests for concrete receivers. Then cover ResponsesClient/Agent and direct client overrides through this same path, and port the shared cases across bridges. Keep the wider callback/protocol work visible; passing this single case does not complete it.


### F31. Owner-defined class projection (metadata verified; codec/facade cutover remains)

The preceding F30 turn made verified implementation progress. Added a shared `ClassProjection` enum and compiler query identifying authored behavior in the defining package. It checks inherent methods and class-headed implementation rules, including generic/out-of-body rules and inherited defaults. It excludes static factories, empty/field-only markers, compiler-generated serialization and blanket/downstream extensions from changing a class's default representation. The canonical alias registry identifies builtin carriers; Prompt retains its dedicated codec. A copied outer record is not made live by its fields. Conditional implementations choose a stable class-level projection without making their methods applicable to every type argument.

The decision is serialized in `ExportedType::Class` and VM Class metadata, and carried in `SymbolPool.class_projections`, including source-less packages. `map_symbols` preserves it during native rewrites. Exported methods retain authored-instance provenance for dependency-interface checks. Runtime class/test builders now initialize the new metadata explicitly. Artifact format 4 covers the changed Program/PackageInterface layout; old SDK bytecode and native bridges need rebuilding together. F30's existing native artifacts were not rebuilt or claimed as verification of this changed format.

The broader native check exposed the runtime compiler's mounted-class constructors. Updated those and preserved the original projection during mount relocation. Runtime mounts can also contain field-only source stubs for linking; the projection queries now give the mounted owner metadata precedence over those stubs for both classes and interfaces. A dedicated test proves a stub with no methods cannot turn a live dependency class into a copied record.

Verification:

- Initial compiler check caught a borrowed-name comparison and an unused import; both are fixed. `cargo check -p baml_ide -p baml_compiler2_emit -p bex_engine -j4` passed in **1m13s** (`/tmp/baml-class-projection-consumers-check.log`).
- Initial selected metadata/export tests passed **6/6**; adding downstream-package and serialized-bytecode checks produced **13/13**. The expanded owner-rule cases include conditional generic implementations and cleanup methods and pass. These initial results precede the mount-stub fix.
- Final `cargo nextest run -p baml_db -p baml_ide -p baml_artifact -E 'test(class_projection)|test(interface_export)|package(baml_artifact)' -j4 --no-fail-fast`: **14/14 passed**, 302 unrelated tests filtered, **26.44s compilation / 6.708s execution** (`/tmp/baml-class-projection-verified-final.log`). This includes the source-less stdlib graph/projection comparison and a bytecode artifact round-trip comparing emitted class flags with exported package declarations. The downstream extension test also checks its implementation rule remains exported.
- `cargo check -p baml_ide -p baml_compiler2_emit -p bex_engine -p sdkgen_python_pydantic2 --all-targets -j4` passed in **1m41s** after the runtime-mount constructor fixes (`/tmp/baml-class-projection-alltargets-final.log`). The later mounted-interface precedence change is covered by the final test run above. The separate VM/heap/project check is recorded next.


- `cargo check -p bex_vm -p bex_heap -p bex_vm_types -p bex_project -p sys_native --features sys_native/aws-crypto --all-targets -j4`: **passed, 16.96s** (`/tmp/baml-class-projection-runtime-targets-final.log`). Its initial command omitted the crypto provider required by `bundle-http` in this package selection; reran with the existing AWS provider feature instead of changing unrelated runtime configuration. This checks the affected runtime/test class constructors and the runtime compiler's mount code after the final query changes.
- Final Cargo formatting and `git diff --check` pass. All F31 commands are terminal. No native addon or generated SDK was rebuilt in this metadata step, and no native runtime/SDK test result is claimed for artifact format 4 yet.

This completes the shared metadata prerequisite, not the Python vertical slice. Outbound concrete-object references, generated concrete methods/factories, native input acceptance and direct client/Agent use remain required. Do not make all values live or relax conformance to make the existing factory test pass. Next, consume this metadata in the retained concrete-object role and its codecs, then make the enabled shared factory case pass through actual generated SDK code. Rebuild CLI/stdlib/native artifacts before that integration run. All wider protocol, host registration, ownership, other-language, media and alias gates remain open. No BEP/site changes, snapshots, PR/push or CI run.

### F32. Retained concrete transport verified; generated facade/input roles remain

The typed outbound codec now consumes the owner-defined projection. Live classes and runtime-created classes use the distinct `ConcreteObject` role (`CONCRETE_OBJECT = 21`); specialized stream/function-spec and media codecs take precedence. Copied outer records still encode their fields, retaining any live children. Internal data serialization remains a separate operation. Runtime-created enums retain their existing role for now.

Inbound validation resolves the actual rooted receiver and its generic arguments. It does not trust diagnostic wire types to change either. Concrete-object role validation also rejects a non-class heap object. Three new engine tests pass: factory-to-interface use and shared state after GC; forged generic pins rejected before side effects; and foreign-runtime/non-class rejection. The first run exposed invalid lambda syntax in the test fixture; correcting the fixture produced **3/3 passing**, 16.69s compilation and 6.495s execution (`/tmp/baml-concrete-reference-engine-final.log`).

Python concrete and interface references now share local ownership/copy/close machinery. A concrete receiver currently decodes to a generic `BamlConcreteRef`, with explicit data conversion routed to its issuing runtime. It does not yet decode to a fully generated concrete facade. Native owned named-call helpers retain the issuer/session and consume transferred arguments on rejection.

Verification:

- Native `cargo check -p bridge_python -p bridge_typescript -p bridge_cffi --all-targets -j4` passed in **2m16s** (`/tmp/baml-concrete-reference-native-final.log`), with two existing unused Node probe warnings.
- Actual playground `bridge_wasm` check for `wasm32-unknown-unknown` passed in **3m29s**, including waiting for the build lock (`/tmp/baml-concrete-playground-wasm.log`). This is distinct from the published Web SDK and does not establish its new-role decoder support.
- The editable Python addon rebuilt successfully in **2m02s** (`/tmp/baml-concrete-python-addon.log`). The combined native run completed **71 passed, 2 failed, 157.85s** (`/tmp/baml-concrete-python-tests.log`). Both failures were new fixture mistakes: a missing ConcreteRecord typemap entry and querying the current runtime's pending count after shutdown. The corrected concrete suite passes **11/11, 31.25s** (`/tmp/baml-concrete-python-final.log`); all **62 existing tests** in the initial combined run passed. No runtime check was weakened. The new cases cover factory pass-back, shared state through copies/views, prepared-call ownership, Pydantic records with concrete children, thrown receivers, issuer close/replacement, rejected decode/encode cleanup and failed scheduling with a weakly observed Python callback.
- Python/vendored-Rust protobuf consumers regenerated through the native build; Node schema generation and checked-in distribution copy/tag steps passed. Typed Node concrete decoding and the other consumer ports are still required.

- Native stub generation passed (**4m39s including build-lock waiting**, `/tmp/baml-concrete-python-stubgen.log`), and its emitted stub was copied to `sdks/python/src/baml_bridge/baml_py.pyi`. The two private owned-call helpers have exact Awaitable/encoded-result signatures.
- The CLI rebuild passed (**6m40s including build-lock waiting**, `/tmp/baml-concrete-cli-build.log`). It generated **253 files in 8s** from the shared fixture. Running the actual shared Python tests against that generated output passes **5/5, 0.48s** (`/tmp/baml-concrete-sdk-generate.log`, `/tmp/baml-concrete-sdk-native.log`). The formerly failing FriendlyGreeter factory case now passes at runtime.
- Added `interface_probes/python/generated_concrete_callers.py` as a pending native-type acceptance probe. Pyright on the newly generated SDK reports **seven diagnostics** (`/tmp/baml-concrete-sdk-static.json`): the concrete class is not accepted by the current GreeterRef-only parameter, and `greet`/`label` are absent, with consequent unknown/assert_type diagnostics. This is an unresolved implementation gate, not an expected permanent failure. No casts or suppressions were added. The generated factory annotation still says FriendlyGreeter while runtime decoding returns the generic concrete reference.

- The existing generated interface-only Pyright consumer still reports **zero diagnostics** against this rebuilt SDK (`/tmp/baml-concrete-sdk-interface-static.json`). The new concrete acceptance probe deliberately remains separate so the implemented and pending surfaces are clear.
- Existing engine interface tests pass **20/20, 21.895s** (`/tmp/baml-concrete-interface-engine.log`), including runtime-created interfaces/receivers, exact generic frames, non-class dispatch and retained defaults. Cargo reported **7m49s including build-lock waiting** for their build.
- C header regeneration through the ignored nextest writer passed, adding the `CONCRETE_OBJECT = 21` discriminant to the checked-in public header (`/tmp/baml-concrete-c-header-generate.log`; 8m08s build including lock waiting, 0.373s writer test).

- The C header match and public media/handle discriminant checks pass **2/2, 0.327s**, with 45 unrelated tests filtered (`/tmp/baml-concrete-c-header-check.log`; 2m00s build including lock waiting).
- The existing anonymous and runtime-compiled class crossing tests pass **2/2, 2.309s**, with 10 unrelated tests filtered (`/tmp/baml-concrete-dynamic-classes.log`; 9m02s build including lock waiting). Their assertions now require the ConcreteObject role while preserving the same-name static/dynamic declaration distinction.
- Cargo formatting, Ruff on the changed Python modules/tests/probe, and diff-whitespace checks pass. All commands from this checkpoint are terminal. No snapshots, BEP/site changes, PR/push or CI run. Long reported Cargo durations above include serialized build-lock waiting; the focused test executions were short.

Remaining before this vertical slice is complete: checked concrete declaration selection for generated facades, public inherent/implemented methods, generic class arguments, distinct configuration construction and typed interface inputs. A diagnostic FQN alone must never select a generated concrete class for a runtime-created declaration with the same name. A generic wrapper making a runtime factory test pass does not prove its generated annotations are correct. All-language consumers, host registrations, provider/Agent use and the broader protocol gates remain open. The BEP and website remain unchanged.

### F33. Compiler-resolved concrete caller contracts (shared metadata verified)

F32 made verified implementation progress. Tracing the concrete facade path found that the shared class symbol only carries inherent methods, while the interface graph carries uninstantiated declarations/rules. Neither is a complete concrete caller contract. In particular, a provided implementation may legally narrow its interface signature; generating the interface's wider signature or invoking it under an existential contract would not preserve concrete BAML calls.

Added compiler-resolved concrete method contracts to the shared graph. They use the compiler's actual member enumeration, ambiguity gate and lookup, preserving selected interface identity and provided/default signature. Class and method generic frames remain distinct even when names and indices overlap. Top-level parameter names and optional modes survive normalization for generated keyword calls. In-body implementation methods use compiler provenance to distinguish them from inherent named methods; their location inside a class is not a public `Class.method` dispatch target.

The graph also carries complete implemented-interface views, using the compiler's existing associated-binding/default resolver and rigid-bound admission gate. A class's narrowed method effect does not repin its interface Error. Validate view completeness against exported declarations; preserve required interfaces and methodless markers. Conditional methods/views unprovable for every allowed class argument do not become unconditional native methods or input claims. The original implementation rules remain available for checked specialization APIs. Sort methods, ambiguity sources and views for deterministic source/source-less output. These graph types are generator metadata; this checkpoint does not change the serialized artifact layout or ABI.

Verification:

- The initial source-less comparison failed on ambiguity-source order for `baml.time.Instant.sub`: both implementations were present in opposite enumeration orders. Sorting the export fixed this without relaxing the comparison. Earlier fixture compilation errors and top-level name preservation were resolved before the final run. The intermediate focused run passed **10/10**, 7.31s compilation / 8.388s execution (`/tmp/baml-concrete-api-sorted.log`).
- Complete-view coverage then passed **11/11**, 29.46s compilation / 8.387s execution (`/tmp/baml-concrete-api-views.log`). Four new concrete tests cover provided signature refinement, concrete `Self`, default methods, optional argument names/modes, class/default-method frame collisions, conditional and bounded implementations, ambiguous names, required views, markers and overridden associated defaults.
- Final `cargo nextest run -p sdkgen_python_pydantic2 -p baml_ide -E 'package(sdkgen_python_pydantic2)|test(concrete_export)|test(interface_export)' -j4 --no-fail-fast`: **138/138 passed**, 264 unrelated tests filtered, **8.63s compilation / 9.275s execution** (`/tmp/baml-concrete-api-final.log`). This includes the final completeness checks, the full Python generator unit suite, and a source-less stdlib comparison of signatures, targets, generic frames, ambiguity sources and complete views.
- Scoped Cargo formatting and `git diff --check` pass. All F33 commands are terminal. No native addon/CLI regeneration, new native runtime test result, snapshots, BEP/site update, PR/push or CI run is claimed for this metadata step.

Technical design §11.7 and adjustment F33 now distinguish concrete caller contracts from interface-reference contracts and from complete input evidence. This prerequisite does not fix facade decoding or F32's seven concrete Pyright diagnostics. Next: establish checked static declaration/schema selection (static type tags are name-derived, not schema fingerprints), generate the concrete facade with these methods and class arguments, invoke its concrete contract without widening it to an existential signature, and emit the native input role using exact views. Then rerun the pending concrete static consumer and actual shared generated SDK cases. Record/config construction, provider/Agent/direct client use and all other bridge, registration, ownership, media and alias gates remain open. The one-PR scope and BEP/site hold remain intact.

### F34. Checked concrete facade selection (bundle path verified)

F33 made verified implementation progress. Generated SDKs already ship their exact bytecode bundle, so pair generated concrete classes with that artifact identity. The validated bytecode loader stamps an immutable engine identity; outbound concrete refs carry static declaration evidence captured under the existing heap permit. Runtime-created declarations and source-created engines do not acquire this evidence from their names. No artifact layout change is needed: the loader evidence is runtime metadata, and the exact-bundle digest is independent of source profiling hashes and name-derived type tags.

Python inspects the owned native table during provisional decoding, before selecting a generated class. This read performs no BAML call or heap acquisition and does not activate the reference. Mismatched bundles fail decoding; dynamic/unknown declarations retain the generic wrapper. The typemap carries separate concrete entries and the generator emits its bundle identity. Concrete entries without a bundle identity are rejected at typemap construction. Native codegen of concrete methods/input roles remains pending; the supplemental identity probe uses small explicit subclasses to test selection and cleanup rather than claiming those APIs are generated.

Verification:

- Initial native check caught the new external metadata's unqualified `Arc` name; fixed it. `cargo check -p bex_engine -p bridge_python --all-targets -j4` then passed in **4m02s** (`/tmp/baml-concrete-facade-check2.log`). The new metadata is not part of the serialized artifact/ABI layout; existing external constructors initialize it explicitly.
- `cargo nextest run -p bex_project -p baml_artifact -p sdkgen_python_pydantic2 -p bex_engine -E 'test(bytecode_artifact_tests)|package(baml_artifact)|package(sdkgen_python_pydantic2)|binary(concrete_references)' -j4 --no-fail-fast`: **138/138 passed**, 446 unrelated tests filtered, **2m00s compilation / 6.541s execution** (`/tmp/baml-concrete-facade-tests.log`). This includes the real validated bytecode-loader test distinguishing a static class from a same-named runtime-created class, artifact skew tests, generated bundle identity, the full Python generator unit suite, and existing concrete transport/admission tests.
- Editable Python addon rebuilt in **1m21s** (`/tmp/baml-concrete-facade-addon.log`). Native stub generation passed in **1m51s** (`/tmp/baml-concrete-facade-stubgen.log`), and its new metadata-reader signature was copied into the public `.pyi` with final whitespace normalized.
- The native concrete/interface/result-transfer/typemap suites pass **67/67, 138.02s** (`/tmp/baml-concrete-facade-python-tests.log`). These ran on the rebuilt addon. The later pure-Python missing-bundle guard does not change their existing empty-concrete-map setup; the final supplemental probe exercises the completed selection path.
- CLI rebuild passed in **1m58s** (`/tmp/baml-concrete-facade-cli.log`); fresh generation emitted **253 files in 8s**, with zero identifier renames (`/tmp/baml-concrete-facade-generate.log`). The generated typemap now contains the digest of that exact embedded artifact.
- `interface_probes/python/concrete_facade_identity.py` passes against that fresh generated SDK and real native addon (`/tmp/baml-concrete-facade-native-final.log`). It checks the generated digest against the artifact, correct subclass selection, independent copies/data access, misleading wire names, mismatched bundles, invalid/missing facade entries, a failing constructor, denial of provisional activation and discarded metadata access, generic fallback and source-engine name collision. Failed selections leave no pending transfers or extra table handles. The small explicit subclasses isolate selection; this is not a generated-method or shared parity claim.
- Actual generated shared Python SDK tests pass **5/5, 0.20s** (`/tmp/baml-concrete-facade-sdk-native.log`). Cargo formatting, Ruff and `git diff --check` pass. All commands are terminal. No BEP/site update, snapshots, PR/push or CI run.

Adjustment F34 and technical design §11.7 distinguish exact-bundle pairing from the remaining general declaration-level negotiation. A different artifact can contain compatible schemas; this step does not establish that interoperability. It also does not implement pre-execution SDK/runtime admission: detecting a stale bundle while decoding a result cannot prevent prior body side effects. Next, bind generated functions/factories to their selected SDK runtime and contract at admission, emit concrete facades/methods and their typemap entries using F33's graph, and emit exact interface input roles. The concrete Pyright gate is still unresolved; it was not rerun as a claimed success here. Complete common C/other-language metadata access, interface-ref declaration checks, callback issuer propagation and all broader protocol/registration/ownership/provider/Agent/media/alias gates before the one-PR completion audit.

### F35. Generated calls bind their SDK runtime and codecs (Python path verified)

F34 made verified implementation progress. Python's runtime object was still a zero-sized selector for the current singleton. It now binds a specific transfer session and weak engine reference; native preparation consumes transferred arguments and rejects a closed/replaced binding before invoking BAML. A retained SDK module must not keep a closed engine heap alive. An admitted call upgrades/retains its issuer, and cancellation on this runtime object targets that issuer. Concurrent replacement during initialization is diagnosed rather than silently binding the replacement.

Generated root initialization captures `_RUNTIME`; free functions and static/instance method factories receive it together with the SDK typemap. Encoding uses a scoped typemap for reverse lookups and generic type tokens, and decoding uses the captured map even across suspension. The scoped override restores the previous context on failure. Generated helper names are protected from user declaration/module/member/generic collisions. Full declaration negotiation, optional explicit runtime API design, context-manager cancellation routing and shutdown/drain races remain broader protocol gates.

Verification:

- Editable native Python addon rebuilt successfully in **12.21s** (`/tmp/baml-sdk-binding-addon.log`). Initial fixture failures exposed missing throws annotations, boundary-error classification and two separate callback limitations described below; no runtime validation was relaxed.
- Final binding and identifier suites pass **12/12, 19.76s** (`/tmp/baml-sdk-binding-python-final.log`). The six new binding cases cover sync/async rejection before callback execution, transferred callback release, captured generic/record codecs, result decoding after the default map changes during `await`, failed-scheduling cleanup, and release of a closed engine's callback capture while its SDK binding and function remain alive.
- Existing native concrete/interface/result-transfer suites pass **55/55, 137.70s** (`/tmp/baml-sdk-binding-native-existing.log`).
- Final full Python generator suite passes **128/128, 0.223s execution** (`/tmp/baml-sdk-binding-codegen-verified.log`). New emission coverage includes root initialization ordering, bindings on free/static/instance factories, and user names colliding with the runtime/map helpers. Earlier runs before this test passed 127/127.
- CLI rebuild passed in **22.03s** (`/tmp/baml-sdk-binding-cli.log`), and fresh generation emitted **253 files in 8s** (`/tmp/baml-sdk-binding-generate.log`). The later generator change adds only its unit test. Actual shared Python tests pass **5/5, 0.20s** against the fresh SDK (`/tmp/baml-sdk-binding-shared-python.log`).
- `interface_probes/python/generated_runtime_binding.py` passes on that generated SDK and real addon (`/tmp/baml-sdk-binding-generated-probe.log`). It verifies captured record decoding after changing the default map, sync/async free function and static factory rejection after identical-bytecode replacement, rejected callback-transfer cleanup, and a working fresh binding on the replacement engine. This probe supplements shared fixtures; it does not claim other bridges implement this behavior.
- Native stub generation passed in **53.49s** (`/tmp/baml-sdk-binding-stubgen.log`); copied the generated bound cancellation signature into the public stub. Ruff formatting/checks pass on the touched Python runtime, tests and probe.

Follow-ups exposed by the tests: callbacks currently run on a separate event loop/thread; use a thread-safe barrier to isolate codec selection and keep caller-loop dispatch as an open executor gate. Python `None` returned under a BAML `void` callback contract currently fails validation; the focused binding fixture uses integer acknowledgements. Add null/void callback and interface return cases and repair normalization before claiming the ordinary method result mapping. These passing tests do not settle either issue.

Next: finish concrete facade method generation and dispatch using F33's compiler contracts and F34's checked selection, then exact interface input roles; rerun the pending concrete Pyright consumer without casts. Audit each old global runtime/codec route (callbacks, context cancellation, streams, function specs, runtime values) and port the common binding/admission semantics to the other bridges. Independent runtime construction, shutdown/drain races, declaration negotiation, all host registration/provider/Agent/media/alias gates and the one-PR completion audit remain open. No BEP/site changes, snapshots, PR/push or CI run.

### F36. Concrete implementation dispatch (shared engine path verified)

F35 made verified implementation progress. The concrete caller graph cannot be routed through the existing existential call operation: the latter deliberately exposes the interface's parameter names/modes, input types and result/effect contract. Added a separate engine/VM concrete-implementation binding entry that shares complete-view validation, receiver/world resolution, implementation selection, owner/method frame construction and bound checking. It retains the actual callee contract and concrete Self; existing existential binding keeps its declaration contract and concrete-Self restrictions.

The engine cases check refined input/name/default behavior, concrete result projection, concrete-Self rejection for another implementor, retained receiver/default frames after GC, non-class receivers and method bounds. These run before adding the common bridge target and generated caller API. Neither a method name nor a native annotation can select an arbitrary function pointer or override the receiver's actual type. This is an internal selected-obligation operation, not an added method on generated existential Ref classes.

Added the entry needed by a generated concrete class: it resolves the expected class declaration, compares its actual pointer against the instance's class, and instantiates the compiler-selected interface obligation using the instance's captured class arguments. Method arguments remain separate. The shared type rewrite now accepts runtime declaration heads; captured nominal types do not have to round-trip through names for substitution. Complete conformance and method bounds remain checked in the VM. A fifth case verifies correct class-slot selection, a separate generic method frame, wrong/missing declaration rejection, invalid/unresolved slot rejection, wrong associated pins and retained calls after GC.

Verification:

- Initial `cargo nextest run -p bex_engine -E 'binary(concrete_methods)|binary(interface_views)|binary(interface_handles)' -j4 --no-fail-fast`: **24/24 passed**, **2m40s compilation / 23.385s execution** (`/tmp/baml-concrete-method-contracts.log`). Includes four new concrete cases and 20 existing interface cases.
- `cargo check -p bex_engine -j4` after adding the generated-class entry and generic shared rewrite: **passed, 1m09s** (`/tmp/baml-concrete-class-method-check.log`).
- Final `cargo nextest run -p bex_engine -p baml_type -E 'binary(concrete_methods)|binary(interface_views)|binary(interface_handles)|test(unify::)' -j4 --no-fail-fast`: **63/63 passed**, 615 unrelated tests filtered, **2m32s compilation / 24.144s execution** (`/tmp/baml-concrete-class-method-tests.log`). Includes all five new host-entry/ownership cases and the existing unification tests. A misplaced test insertion inside an embedded BAML string was caught by formatting and corrected before this run; no language or runtime check was relaxed.
- Scoped Cargo formatting and `git diff --check` pass. All F36 commands are terminal. This step did not rebuild the native addons, CLI or generated SDK; it makes no new native-language or static-codegen success claim. No ABI/schema change, snapshots, BEP/site update, PR/push or CI run.

Next: expose the selected concrete target through the common prepared-call/receipt path and Python's issuing-runtime native helper. Carry the exact expected class and compiler interface pattern, pin the concrete receiver before yielding, and let the engine fill class slots from the actual instance. Preserve separate method type evidence. Then emit live facades, checked typemap entries and typed interface inputs; make the concrete Pyright and shared method cases pass without casts or an existential-result conversion. Inherent methods still need their corresponding owned concrete caller path. General dynamic declaration negotiation, independent runtime/scoped ownership, all other bridges, host registrations, provider/Agent/media/aliases and the full one-PR audit remain required.

### F37. Concrete method target across the shared bridge (shared/Python paths verified)

F36 made verified implementation progress. Added an explicit concrete-interface-method target to the common request schema and prepared-call dispatch. Preparation pins the actual ConcreteObject entry, parses the class and interface pattern, keeps positional method evidence separate, and consumes the transferred argument batch on rejection. BexProject routes it to F36's checked class binding and invokes the resulting callable with names validated against its concrete contract.

Python's concrete reference uses the existing issuing-runtime owned-call helper and the common result receipts. The Python encoder accepts the target directly inside its rollback boundary; the wrapper does not rewrite bytes after transferring argument ownership. Encoding uses the receiver's typemap. A small native accessor checks adoption and concrete role before borrowing the target key; actual table-role, class and conformance validation still happen during preparation/admission. The existing callable-only key accessor stays callable-only. Typed facade emission and exact interface input roles remain open.

Verification:

- Initial checks exposed a private engine keyword-call helper and a missing qualified-name import through the BexProject boundary. Exposed the checked keyword operation and re-exported the existing TypeName rather than interpreting argument positions in adapters or adding a new name representation.
- `cargo check -p bridge_cffi -p bridge_python -p bridge_typescript --all-targets -j4`: **passed, 50.77s** (`/tmp/baml-concrete-method-bridge-check3.log`), with two existing unused Node probe warnings. This precedes the later small Python concrete-key accessor, which was compiled by the addon/stub builds below.
- Final `cargo nextest run -p bridge_cffi --test interface_calls -j4 --no-fail-fast`: **11/11 passed**, **6.55s compilation / 8.911s execution** (`/tmp/baml-concrete-method-wire-final.log`). The initial 10-case run passed before adding the last ownership case. Three new cases exercise concrete names/defaults and prepared-receiver survival, whole argument-batch cleanup on invalid target/role/pattern, and live method type evidence plus interface arguments surviving release of their original SDK keys.
- Rust/prost and vendored Rust/Python schema files regenerated through the native build. Node `build:proto`, `build:copy-proto`, and `build:tag-generated-files` passed; the checked-in source and distribution schemas contain the new target (`/tmp/baml-concrete-method-node-schema.log`). This does not establish native method API parity or complete remaining-language schema generation.
- Editable Python addon rebuilt in **2m05s**, then **6.40s** after adding the concrete-key accessor (`/tmp/baml-concrete-method-python-addon.log`, `/tmp/baml-concrete-method-python-addon2.log`). The first native run passed 39 cases and failed four new cases because the wrapper incorrectly used the callable-only key accessor. The next run passed 14 concrete cases and exposed two test expectations using BamlError where the decoder intentionally returns native TypeError for membership failures. Corrected the accessor and exact expected error categories; no admission check was relaxed.
- Final concrete-reference suite passes **18/18, 53.75s** (`/tmp/baml-concrete-method-python-verified.log`). A subsequent record/typemap case passes **1/1, 2.57s**, with the prior 18 deselected (`/tmp/baml-concrete-method-python-map.log`). The resulting 19-case suite covers actual default/generic calls, owner-side state, prepared calls after local close, cleanup on target/encoding/scheduling failure, shutdown/replacement rejection and callback release, class-slot validation, and captured record codecs after replacing the process-default map. All 27 existing result-transfer cases passed in the initial combined run; the later native change only adds the concrete-role key accessor.
- Native stub generation passed in **3m01s** (`/tmp/baml-concrete-method-python-stubgen.log`); copied the generated concrete-key signature into the public `.pyi`. Scoped Cargo formatting, Ruff and `git diff --check` pass. All F37 commands are terminal. No CLI regeneration, new generated SDK/static-check success, snapshots, BEP/site update, PR/push or CI run is claimed.

Next: emit concrete live facades and checked typemap entries from the compiler graph, including public methods and factories, then exact interface input roles. Route interface-provided/default methods through this concrete target; complete inherent method invocation using the same retained receiver/class-frame discipline. Preserve native method/helper name collisions, configuration construction and generic class tokens as explicit codegen requirements. Make the existing concrete Pyright consumer and shared method cases pass without casts or manual interface conversion. Remaining schema consumers/native helpers, all other bridge/host-registration/ownership/provider/Agent/media/alias gates and the one-PR audit stay open.

### F38. Inherent concrete methods (shared/Python path verified)

The concrete graph includes methods declared directly on classes as well as interface implementations/defaults. Extend the prepared concrete call target with an explicit inherent/interface selector and keep both on the issuing-runtime ownership path. The engine validates the exact receiver class, uses its captured generic arguments, and checks the separately supplied method arguments. Static members and interface bodies cannot be selected as inherent receiver methods.

Verification:

- `cargo check -p bridge_python -p bridge_typescript -p bridge_cffi --all-targets -j4`: **passed, 1m10s** (`/tmp/baml-concrete-inherent-check.log`); only the two existing unused Node probe warnings remain.
- `cargo nextest run -p bex_engine -p bridge_cffi -E 'binary(concrete_methods)|binary(interface_calls)' -j4 --no-fail-fast`: **18/18 passed**, **2m22s compilation / 16.474s execution** (`/tmp/baml-concrete-inherent-tests.log`). New cases cover owner-side state, captured class types, independent method arguments, GC retention, invalid argument/type-argument counts, wrong classes, static/interface member rejection and inherent wire preparation. Missing/false dispatch rejection releases the complete argument batch.
- Python editable addon rebuilt in **48.23s** (`/tmp/baml-concrete-inherent-addon.log`). All concrete native reference tests pass **20/20, 54.30s** (`/tmp/baml-concrete-inherent-python.log`), including new inherent mutation/generic/invalid-input and pending-call-after-close coverage.
- Node protobuf generation/copy/tag completed. Both source and dist schemas round-trip the concrete target and explicit inherent selector in a Node smoke check. Rust/Python generated schemas also carry the new target. This is schema verification, not generated Node caller execution.
- Scoped Cargo formatting, Ruff and diff-whitespace checks pass. All commands are terminal. No CLI/generated SDK rebuild, new static-codegen success, snapshots, BEP/site update, PR/push or CI run is claimed.

`INTERFACES_CODEGEN_REVIEW.md` now records the multi-instantiation constraint for native input roles. A Python invariant proof token with per-view overloads is a candidate to probe, not a frozen/generated API. General dynamic declaration negotiation and non-static inherent callers remain broader gates.

Next: emit the concrete methods and facade typemap entries from the shared compiler graph, then rerun the pending generated concrete consumer without casts. Native input roles must preserve complete interface bindings (including multiple instantiations of one generic interface); ordinary method-shape protocols are not sufficient proof. The BEP/site hold and the one-PR scope remain unchanged.

### F39. Python concrete facade generation (generated path verified; input role pending)

Consume the compiler-owned Live projection and concrete method graph in Python generation. Live classes derive from the retained concrete base, omit copied fields, keep static factories, and emit the selected inherent/implementation/default methods. Class and method variables are rewritten by identity before native naming. Concrete methods require explicit own `_types` for now, matching the checked binder; class arguments come from the receiver. Map live facades separately from Pydantic records. Preserve dedicated builtin projections.

The generated method returns the owned-call coroutine after synchronous preparation, so closing the source wrapper cannot invalidate an admitted call. Its stub exposes the async result contract. Encode interface patterns with the shared semantic wire encoder rather than a Python-generator-specific type switch. The direct `bridge_ctypes` dependency currently also brings engine dependencies into the generator; assess a pure shared wire layer before treating this as the final cross-generator dependency structure.

A source-only SDK currently has no checked bundle identity, so generation with live concrete entries is rejected instead of emitting an SDK that cannot select its annotated facades. Supporting an equivalent checked source-initialization identity remains open. No source-only support or overall facade/input completion is claimed.

The actual generated probe found three integration gaps and drove fixes:

- Method lookup exports a declaring obligation without every associated pin. Match it to the unique complete view already proved by the compiler, preserving defaults/overrides. Fix this once in `baml_ide` rather than reconstructing pins in a native generator. Source-less and generic-frame coverage now asserts complete method targets.
- `ai.clients.ClientSelector` eagerly references `ai.ClientRef` during a circular import. Emit interface Ref definitions before the leaf's eager alias/field imports; emit exports after the leaf body so `__all__` remains complete.
- A generated live generic class uses `typing.Generic`, not Pydantic specialization. Encode its origin and class arguments explicitly. Use separate annotation lookup for nested generic records, preserving the checked value-decoding gate.

Verification:

- Initial checks caught a moved diagnostic-name borrow and a test mutation through Arc; both were fixed. The first full generator run passed **129/129**, **7.73s compilation / 0.246s execution** (`/tmp/baml-concrete-codegen-tests2.log`). The new test checks separate generic frames, method/helper collisions, concrete/default selectors, omitted fields and separate typemap entries.
- After completing associated targets, focused compiler-export plus full Python generator tests pass **140/140**, **2m07s compilation / 8.917s execution**, 264 unrelated tests filtered (`/tmp/baml-concrete-complete-targets-tests.log`). After import ordering changed, the full generator suite again passed **129/129, 0.220s execution** (`/tmp/baml-concrete-import-order-tests.log`).
- Native concrete/identifier suites pass **26/26, 57.77s** (`/tmp/baml-concrete-codegen-native.log`). The subsequent generic-token change is exercised by the actual generated end-to-end probe below. No new native addon ABI was required beyond F38's rebuilt addon.
- CLI builds passed in **2m59s**, **33.68s**, and finally **21.06s** as the discovered gaps were fixed. Final generation emitted **253 files in 8s**, zero identifier renames (`/tmp/baml-concrete-import-order-generate.log`). All subsequent consumer checks use that output.
- `interface_probes/python/generated_concrete_methods.py`, generated with `interface_probes/baml/concrete_facades.baml` alongside the shared fixture, **passes** (`/tmp/baml-concrete-generated-methods-verified.log`). It executes real `ResponsesClient` construction/id and direct concrete-client input to `Agent.new_async`, without a network request. It also covers generic/default/inherent calls, concrete Self returns, owner-side mutation, generic copied records with live children, captured type tokens/codecs after changing the default map, invalid-input rejection and admitted-call/copy ownership after close. This is not an `Agent.run`, LLM override, streaming or all-bridge claim.
- The shared generated Python suite passes **5/5, 0.23s** in the initial fresh run and **5/5** on final output (`/tmp/baml-concrete-generated-shared-final.log`). Its existing concrete-input case now also checks the actual generated facade class and its provided/default methods. Other language ports still must satisfy the same semantic gate.
- The supplemental facade identity/cleanup probe passes (`/tmp/baml-concrete-generated-identity.log`). It uses custom subclasses to isolate checked selection and is distinct from generated-method coverage.
- The new generic concrete typing consumer and the existing interface-only consumer both report **zero Pyright diagnostics** (`/tmp/baml-concrete-generated-generic-static-final.json`, `/tmp/baml-concrete-generated-interface-static-final.json`). The original concrete acceptance consumer now has **one remaining diagnostic**, down from seven: `FriendlyGreeter` cannot yet satisfy a `GreeterRef`-typed input (`/tmp/baml-concrete-generated-static-final.json`). Do not hide it with a cast or change that acceptance probe. An initial generic-probe config used an absolute include, which Pyright ignored; the corrected config checks one explicit relative consumer file.
- Scoped Cargo formatting, Ruff and diff-whitespace checks pass. All commands are terminal. No snapshots, BEP/site update, PR/push or CI run.

Next: emit exact interface input roles, including multiple instantiations of one interface, and eliminate that remaining static diagnostic. Then finish direction-aware nested input/callback types, conditional specialization, host implementations and method inference needed by `Agent.run(spec)`. Source-only SDK identity, configuration construction, the common wire dependency split, all other bridge ports, client overrides/streaming, media/aliases and the one-PR completion audit remain open. Do not equate these verified Python caller/factory paths with completion of the full design.


### F40. Python input roles (partial verification; copied-record admission fails)

Existing work emits `InterfaceInput` protocols with private invariant witness types. Complete compiler-proven views generate matching proof methods on Ref/live facade/record classes; multiple instantiations use overloads. Derived names share the identifier allocator. Native caller parameters use Input for direct interface heads and unions; returned values remain Ref. This is an initial projection, not complete recursive input support or host implementation registration.

This continuation reviewed the design feedback and ran existing probes. It changed internal documents only, preserving implementation files and the BEP/site hold.

Evidence:

- The prior focused compiler-export/full Python generator run passed **140/140**, **15.43s compilation / 9.168s execution**, 264 skipped (`/tmp/baml-interface-input-tests.log`). These existing tests do not independently cover all new input promises; add dedicated assertions below.
- The prior CLI build completed in **48.73s** and generation emitted **253 files in 8s**, with one intentional identifier rename (`/tmp/baml-interface-input-cli.log`, `/tmp/baml-interface-input-generate.log`). No matching build/generation process remained at review time.
- Pyright checks one explicit relative consumer per config against that generated SDK. `generated_input_types.py` reports **zero diagnostics** (`/tmp/baml-input-types-result.json`). The original, unchanged concrete acceptance consumer also reports **zero diagnostics** (`/tmp/baml-input-original-concrete-result.json`), closing F39's FriendlyGreeter annotation gap.
- `generated_input_rejections.py` reports exactly **five errors**, one at each marked line and none elsewhere (`/tmp/baml-input-rejections-result.json`). Cases: Error alone differs (`never` versus `string`), item type differs, unrelated concrete class, ordinary method-shaped object, and unrelated marker-interface input. These are expected negative checks, not an all-green executable fixture.
- The shared generated Python suite passes **5/5, 0.20s** (`/tmp/baml-input-shared-result.log`). No parity fixture names were added by this documentation review.
- `generated_interface_inputs.py` **fails** at `use_tagged_async(TaggedRecord(name="Ada"))` (`/tmp/baml-input-runtime-result.log`). Native typing accepts the generated record, but runtime reports that `TaggedRecord` is not assignable to `Tagged`. Earlier dual-instantiation and nullable live-input assertions execute successfully; later assertions in that failed run are not evidence.
- A separate temporary native check verifies both live instantiations, nullable input, rejection of an unrelated object copying private proof methods, actual ResponsesClient factory/id and direct input to Agent.new_async. It passes and ends with **zero pending transfers**. It deliberately does not claim the failing record case passed, nor does it establish general leak freedom, Agent.run, overrides or streaming.

The failure is in the inbound annotated-data path in `bex_engine/src/conversion.rs`: its early assignability check does not have the implementation-rule context needed to admit this concrete record as the interface. Do not bypass annotation/field validation, trust the generated private marker, or force record classes to become live to avoid the failure. Validate the concrete data and its declaration, obtain checked membership, and retain the resulting BAML-owned receiver only as needed.

Next steps, in order:

1. Add focused shared admission tests for generated record-to-interface inputs: valid marker/default methods, exact generic/associated pins, wrong pins before body execution, malformed fields, and provisional nested-reference cleanup. Include interface positions inside unions. Repair the common admission path and rerun the unchanged failed probe.
2. Add compiler/generator assertions for record implementation views, two instantiations, derived-name collisions, invariant error-only pins and source-less export agreement. Keep runtime and native type-check evidence separate.
3. Add substituted required-interface proofs, including Iterator-to-Iterable and wrong Error rejection. Complete conditional specialization, aliases, nested record/container input shapes and callable direction without widening invariant containers.
4. Add equivalent semantic cases to shared sdk_tests and run their native ports. Supplemental Python probes do not satisfy the parity gate. Finish host registration/ordinary versus advanced handlers and remaining backend projections before publishing support claims.

All review commands are terminal. No implementation changes, snapshots, BEP/site changes, PR/push or CI run were made in this continuation. The full single-PR goal remains incomplete; no language or interface scope has been cut.


### F41. Admit copied nominal implementations through the shared runtime

Resolve a copied record's source class and complete generic arguments in the active runtime, then ask the VM whether that source implements the retained interface contract. Materialize under the concrete source schema and validate the actual resulting value too. Interface-containing unions accept this exact source evidence; arbitrary method-shaped objects and untyped collections acquire no new admission path. Records stay copied. An interface returned for the copy retains that BAML receiver independently of later native edits.

Host callback completion uses the same source-membership rule, with strict validation before argument coercion. The native and WASM transport return guards previously rejected these inputs using a schema-free, name-only subtype check. Remove those duplicate guards: every host-call sysop already captures the engine's exact return/throws contract and validates completion before resuming. Scalar, JSON, field and error checks remain enforced by that engine path. Four native helper-only tests were removed with their redundant helper; actual engine callback tests exercise the validation boundary.

Verification so far:

- Initial focused copied-input run: **2/3 passed**. The failing test wrongly assumed that a class acquiring default methods would remain Record. The existing defining-package projection correctly makes it Live. Changed the copied-record fixture to a marker interface; no projection rule changed.
- Full interface-view suite: **18/18 passed**, **49.97s compilation / 17.774s execution** (`/tmp/baml-copied-interface-tests2.log`). New cases cover direct/annotated generic records, nullable interface inputs, wrong Item/Error pins, missing/extra class type arguments, malformed fields, pre-body rejection, retained views and live children.
- First copied host-result probe exposed the redundant transport guard. After removing it, the conversion + full interface-view + full host-callable selection passed **139/140**, **16.12s compilation / 51.029s execution** (`/tmp/baml-copied-interface-completion2.log`). The remaining assertion expected a copied UnknownError although the existing behavior projection now retains it. Updated that test to assert a concrete receiver and inspect its original HostCallable data/message in BAML, preserving the original trace assertion. Focused final cases then passed **5/5**, **8.65s compilation / 6.327s execution** (`/tmp/baml-copied-interface-final.log`). No return or throws check was weakened to pass this test.
- Python addon rebuilt successfully in **39.84s** (`/tmp/baml-copied-interface-addon.log`). The unchanged `generated_interface_inputs.py` now passes (`/tmp/baml-copied-interface-python-final.log`), including the formerly failing marker record, dual interface bindings, forged-marker rejection, actual ResponsesClient factory/id and direct Agent factory input. The earlier generated concrete-method probe also passes (`/tmp/baml-copied-interface-concrete.log`). These are offline factory/caller checks, not Agent.run or provider/streaming validation.
- Added shared `interfaces/copied_record_implements_interface_input` and its BAML fixture. Python, TS and Rust ports assert that editing the original native record cannot modify its retained BAML copy. Fresh Python generation and native execution pass **6/6, 0.28s** (`/tmp/baml-copied-interface-shared-final.log`). The TS/Rust declarations remain acceptance cases awaiting their generated interface APIs; they were not executed here.
- The new compiler-export test verifies Record projection, marker membership, both complete Mapper instantiations with exact Error pins and the ambiguous concrete method name. The first attempt had two test-code API errors; after correction, it exposed an invalid total-view-count assertion because auto-derived interfaces are also exported. Filter the specific marker view instead of discarding other implementations. Final focused test passes **1/1, 2.322s**, after **1m08s compilation** (`/tmp/baml-copied-interface-export-final.log`). The preceding combined run passed the other nine compiler-export/native-transport tests, including source-less export (`/tmp/baml-copied-interface-export-native2.log`).
- Full parity lint completed and wrote `/tmp/baml-interface-parity-f41.md`. Its aggregate baseline comparison passes, but the new row still lacks Go, C#, Java, Swift and C++ declarations. Python, Rust and the three TS environments are present as declarations, not verified executions. Do not update the baseline or count aggregate improvement as satisfying this row.

- `cargo check -p sys_wasm --target wasm32-unknown-unknown -j4` passes in **3m08s**, including time waiting for the earlier compiler build (`/tmp/baml-copied-interface-wasm-check.log`). This type-checks the actual WASM target; it is not browser callback execution.
- Strengthened the failed-copy test so an unannotated nominal input materializes a valid live child before a later field fails. The final copied-input selection passes **3/3, 3.720s**, after **1m57s including the WASM build-lock wait** (`/tmp/baml-copied-interface-partial-cleanup.log`). Final GC returns active handles to baseline for both retained successful copies and partial failures.
- Scoped Rust formatting, Python Ruff and diff-whitespace checks pass. All commands are terminal. No snapshots were changed in this checkpoint.

Next: finish required-interface input proofs and recursive directional native types, and port the new shared case to the five missing languages. Add generated static/codegen checks where the existing supplemental probes are not yet harnessed. Recursive native input shapes, required-interface proofs, conditional implementations, host sources and all other language/runtime/ownership/media/alias/client gates remain open. BEP/site remain on hold; no PR/push or CI run has been made.

### F42. Required-input evidence is separate from inherited method dispatch

This documentation continuation records existing work and checks the corresponding source and logs. No implementation files or public BEP/site were edited in this continuation.

The shared export now normalizes transitive requirement arguments and associated bindings using compiler facts. Python emits exact required-view evidence on both Ref and Input roles, in the required declaration's parameter/associated order. Missing associated pins do not acquire declaration defaults. Private proof overloads deduplicate identical native witnesses. The fixture `interface_probes/baml/required_interfaces.baml` exercises Root → Middle → Base with intermediate associated projections and an unpinned requirement.

Existing verification, confirmed from its terminal output and saved results:

- Compiler-export selection plus full Python generator: **137/137 passed**, **26.74s compilation / 6.487s execution**, 270 skipped (`/tmp/baml-required-input-tests.log`). Includes transitive normalization, retained symbolic projections, missing pins and generated proof order on both input/ref roles. The running-session result was harvested during this review.
- Updated generation: **253 files in 8s**, one intentional rename (`/tmp/baml-required-input-generate2.log`). The initial generation exposed an unresolved intermediate associated projection; the compiler normalization repaired it.
- `generated_required_input_types.py`: **zero Pyright diagnostics**, one file analyzed (`/tmp/baml-required-input-types.json`). Includes transitive input acceptance and IteratorInput → IterableInput.
- `generated_required_input_rejections.py`: **five expected errors**, one at each marked line and no other diagnostics (`/tmp/baml-required-input-rejections.json`). Covers wrong associated Error, wrong Output/Item and a requirement whose associated bindings remain unspecified.

Source review exposes the next gap: `IteratorRef` still lacks inherited `iter()`, and `bex_vm::bind_checked_implementation_method` searches the named declaration's own method list. Successful required-input typing cannot be counted as inherited invocation support. The codegen review and design §9 now state both promises explicitly.

Next bounded implementation steps, when implementation work resumes:

1. Export compiler-resolved inherited caller contracts and declaring views, including root shadowing, realized-declarer deduplication, ambiguity and concrete-Self restrictions. Reuse language resolution rather than implementing independent precedence in each generator.
2. Make shared checked invocation consume the correct declaring view while retaining the original receiver, implementation world and exact pins. Wrong or ambiguous targets must fail before a method body runs.
3. Generate direct inherited callers, including `await iterator.iter()`. Verify success/result/effect types independently of input acceptance. One view must not copy state or create a new host registration.
4. Add shared sdk_tests for transitive required inputs, inherited calls, shared iterator state, wrong Error before state changes, diamond resolution and retained calls after closing the originating SDK ref. Add exact native typing cases in each supported backend and check the new parity rows individually.
5. Continue recursive input roles and the broader unresolved ownership, registration, client/Agent, media, alias and backend gates. Do not claim these supplemental Python checks close any all-language checkpoint.

No new test execution, snapshot update, PR/push or CI run was needed for this documentation pass. Existing evidence is deliberately distinguished from the proposed inherited API.

### F43. Resolve inherited caller contracts and runtime targets together

Implementation resumed under the active goal. Shared SDK metadata now separates `InterfaceDeclaration.callers` from its own required/default declarations. Caller signatures use the compiler's member resolution, normalized transitive associated projections and a rebased method-generic suffix; own host implementation obligations remain unchanged. Ambiguous names retain their declaring candidates. The emitter uses the same compiler member lookup to bake named declaring-interface templates into `InterfaceDef.method_dispatch`; runtime binding realizes and checks that target on the existing receiver instead of searching only the root's own method list. The shared head walk visits these templates for relocation and GC.

Python generation now consumes the resolved caller list for both method names and signatures. Runtime wrappers prepare calls before returning their awaitables, consistent with concrete wrappers; stubs keep the ordinary async caller signature. Interface argument/type-token encoding now uses the receiver's typemap, with type-token selection shared on BamlRef. Added shared inherited-counter/state and Python admitted-call release cases; backend declarations are not execution evidence.

Verified so far:

- `cargo check -p baml_ide -j4`: passed, **1m06s**. The first test build caught two uses of a nonexistent test-only `RuntimeTy::never()` constructor; replaced with the actual variant.
- `cargo nextest run -p baml_ide -p sdkgen_python_pydantic2 -E 'test(symbol_pool::interface_) | package(sdkgen_python_pydantic2)' -j4`: **142/142 passed**, **9.71s compilation / 9.422s execution**, 268 skipped (`/tmp/baml-interface-callers-tests2.log`). This precedes switching Python emission to the caller list; repeat after that change. New compiler tests cover transitive/unpinned associations, overlapping generic frame names/indices, diamonds, shadowing, ambiguity and concrete-Self/receiverless classification. Full source-less export equality includes the new caller metadata.
- `cargo check -p bex_engine -p baml_compiler2_emit -j4`: passed, **50.81s** (`/tmp/baml-inherited-dispatch-check.log`).
- `cargo nextest run -p bex_engine --test interface_views -j4`: **21/21 passed**, **1m24s compilation / 20.422s execution** (`/tmp/baml-inherited-dispatch-tests.log`). New cases cover retained bound inherited calls and GC baseline, transitive associated dispatch, generic inherited calls, diamonds, ambiguity, root shadowing and root-view preservation for a bare Self return. The runtime-created-interface fixture has subsequently been extended to inherit `label()`; rerun that case.

Final verification:

- Added dispatch templates change the serialized Program layout, so artifact format is now **5**. CLI builds passed in **2m17s** initially and **1m35s** after the format bump (`/tmp/baml-inherited-cli{,2}.log`). No compatibility decoder was added.
- The first workspace-root `maturin develop --uv` command stopped before compilation because its dependency-group installer searched the root for a Python pyproject. Building with `--skip-install` succeeded in **1m46s** (`/tmp/baml-inherited-addon2.log`). That command left the editable module unchanged, so the freshly built `target/debug/libbaml_py.dylib` was explicitly copied to the existing editable module path before tests. Native interface/concrete-reference suites pass **37/37, 97.53s** (`/tmp/baml-inherited-native-tests.log`).
- The shared fixture plus pinned transitive probe generates **253 files in 10s**, no identifier renames (`/tmp/baml-inherited-generate.log`). The generated Python shared suite passes **8/8, 0.30s** (`/tmp/baml-inherited-shared.log`), including inherited counter calls, state through a required parent view, and a call awaited after closing its originating wrapper.
- `interface_probes/python/generated_inherited_callers.py` executes successfully against that SDK and ends with zero pending transfers. It exercises transitive `apply`, inherited generic `echo`, admitted-call retention and direct `iterator.iter()` sharing the existing cursor. This is limited ownership evidence, not general cross-language cycle collection. Pyright reports **zero diagnostics** (`/tmp/baml-inherited-types2.json`); its first run complained only about the private test counter import, now suppressed at that import alone. `generated_inherited_rejections.py` reports exactly **three errors** at its marked lines: wrong inherited argument, wrong associated result assignment, and wrong required Error pin (`/tmp/baml-inherited-rejections.json`).
- The expanded combined run initially passed 33 cases and failed the new iterator assertion because the engine correctly preserves the declared Item/Done union envelope. Corrected the assertion to check the selected member and payload. Final compiler export, complete Python generator, complete interface-view suite, artifact and selected GC tests pass **171/171**, **7.45s compilation / 27.961s execution**, 831 skipped (`/tmp/baml-inherited-final-tests2.log`). This includes inherited `label()` from a runtime-created package and both interface/default and implementation-rule GC edge tests.
- Exact parity report: `inherited_method_preserves_receiver_state` has Python, Rust and three TS environment declarations; it lacks Go, C#, Java, Swift and C++. `inherited_call_survives_reference_release` currently has Python only. The aggregate lint improves, but neither row has complete parity (`/tmp/baml-interface-parity-f43.md`). Rust/TS cases remain unexecuted acceptance declarations; no baseline was changed.
- A separate full unpinned probe **fails generation**, exit 101, at `user.RequiredUnpinned.apply` because its associated projection has no CodegenTy mapping (`/tmp/baml-inherited-unpinned-generate.log`). The successful SDK above deliberately includes only the pinned part; it is not evidence that the full F42 fixture still generates. Preserve this unresolved member and its constraint in shared metadata. Implement a checked specialization/dynamic caller path and a useful generation diagnostic rather than fabricating defaults, emitting Any, silently omitting the method or declaring that interface unsupported in BAML.

All started processes are terminal. Scoped Rust formatting, Python Ruff, BAML formatting and diff whitespace checks pass. No snapshots, BEP/site, PR/push or CI changes were made in this checkpoint. Keep the source-only SDK, other-backend, host registration, ownership, media, alias and client/Agent gates open; inherited invocation is a verified step toward the full goal, not completion.

### F44. Select missing associated bindings through checked interface projection

The F43 unpinned case needs an explicit fully specified declaring view. Python's proposed `await value.as_interface(TargetRef[...])` accepts a generated interface Ref type from the receiver's SDK, preserves its native generic result type, and rejects unrelated/subclass-fabricated facade types. It runs `baml.identity<T>(value: T) -> T` through the existing owned typed-call path. Argument admission checks membership; the return retains that exact interface view of the same receiver. No additional wire target, registration or ownership protocol is introduced. Concrete generated client inputs remain automatic.

The ordinary generator now identifies the narrow case where an inherited caller needs unspecified associated types of its own declaring interface. It keeps the method in compiler metadata, documents the checked projection in the generated Ref docstring and `INTERFACE_PROJECTIONS.md`, and exposes the callable on the selected fully specified Ref. Other unresolved projections, including method-generic dependent types or a projection not supplied by that declaring view, do not acquire this fallback. They still require their own checked API. New reserved helper names preserve authored `as_interface` methods through normal name allocation.

Added shared Python projection cases for exact Value/Error pins, shared state, independent release and a projection awaited after its originating wrapper closes. A focused engine test checks the actual typed identity path and final GC baseline. At the initial checkpoint, native generation/execution, positive/negative typing, new generator classification checks and parity review were still in progress. The initial existing compiler/Python generator selection passes 142/142; it precedes the new classifier test. The subsequent generation probe includes the full unpinned fixture, not filtered out as in F43. BEP/site remain unchanged.

F44 verification completed in the implementation continuation:

- The focused engine typed-identity test passes 1/1 (2.941s execution), and the new generator classification test passes 1/1 (0.010s). Logs: `/tmp/baml-interface-projection-engine.log`, `/tmp/baml-interface-projection-classifier.log`.
- The full fixture, including the unpinned required interface, generates 254 files successfully. The generated native projection consumer runs successfully against the rebuilt Python addon (2m41s addon build). Generated positive typing has zero diagnostics across three consumers; negative typing has exactly eight intended errors across two consumers, including the three new target/argument/result failures. Logs: `/tmp/baml-interface-projection-generate.log`, `/tmp/baml-interface-projection-positive.json`, `/tmp/baml-interface-projection-negative.json`.
- The generated shared Python suite passes 10/10 in 0.22s. Existing native reference/concrete suites pass 37/37 in 94.95s. Three additional native projection checks pass 3/3 in 10.80s: captured typemap, repeated accepted/rejected projections returning to the handle baseline while the singleton remains open, and replaced-issuer rejection. These are separate selections, not one full-suite 40-test run. Logs: `/tmp/baml-interface-projection-sdk.log`, `/tmp/baml-interface-projection-refs.log`, `/tmp/baml-interface-projection-ownership.log`.
- Parity lint exits 1. Both `checked_interface_projection_preserves_receiver` and `checked_interface_projection_survives_reference_release` have Python declarations only; the other nine environments remain missing. No baseline adjustment. Exact report: `/tmp/baml-interface-parity-f44.md`.

The declaring-view specialization is verified for this Python path; arbitrary dependent projections and other native backends remain open. The pure `baml.identity` helper also enters ordinary free-function generation. General explicit generic choices must preserve the native argument/result relationship; this existing generic-codegen gate is not closed by the typed `as_interface` helper. No BEP/site, PR/push or CI changes.

### F45. Preserve typed unhandled errors and their issuing authority

Unhandled error aggregates previously owned their data but attached no runtime to decoded references. Carry the original runtime and its admission session in the owned error callback/queue. Capture the runtime weakly in its installed reporter to avoid a runtime→reporter→runtime cycle. Upgrade only for a delivered error, whose owners release the runtime with the aggregate/references.

Receipt adoption and invocation admission must be separate for this path: errors can be reported after shutdown. A fresh delivery receipt remains adoptable; retained Python refs consult the original (possibly closed) admission session. Node now retains the same metadata, but its reference-method port remains incomplete. Host-callable argument delivery still needs its own issuing authority; this change does not repair that separate path.

New Python cases cover invoking a thrown interface while its issuer is open, and decoding then rejecting invocation after shutdown/replacement. The initial probe waited for a report without collecting the unobserved failed future; allocation pressure also did not establish delivery. Added a private runtime GC maintenance hook, tied to that runtime, so ownership tests can force real collection without depending on allocation heuristics or shutdown. The initial common callback registry selection passed its three ownership/handoff tests before the maintenance-hook addition. Final evidence follows.

The native probe then exposed a second problem: the error arrived as a copied `user.Counter` record because reporting used `vm_value_to_owned`, the trace conversion. GC now retains the settled future (including its existing exact throws-type head edges) until the engine exports the value using typed boundary conversion. This preserves an interface view rather than reconstructing one from a class name. Pending reports stay in the engine-owned queue across permit acquisition, so cancellation during that await cannot discard the roots. Panic values keep the foreground-call exception to the authored throws contract. No new serialized artifact or wire shape is introduced.

Verification:

- Engine, GC type-head and common registry selection passes 33/33 in 16.023s (7.68s incremental build), including the new minor/major-GC interface error test and wrong Error-pin rejection before mutation (`/tmp/baml-interface-error-typed-gc3.log`). The previous run had 31 passes and one old assertion expecting an unwrapped string. Its spawn can also throw sleep's Io error, so the updated assertion checks the preserved declared union and string payload. The final engine background-task suite also passes 27/27 in 15.791s (1m08s build) after the queue-acquisition ordering change (`/tmp/baml-interface-error-final-engine.log`). This repeats the engine subset; do not add the counts together.
- Final rebuilt Python addon (42.62s build): background-error, runtime-binding, interface-ref and concrete-ref suites pass 55/55 in 158.44s (`/tmp/baml-interface-error-final-python.log`). They include open/shutdown/replaced issuer cases and the existing weak-binding heap-release check. The earlier focused three-case native run passes in 10.32s after typed conversion; earlier failed probes are not counted as successful validation.
- Full generated Python SDK still emits 254 files. The expanded shared SDK fixture passes 11/11 in 0.25s, including `spawned_interface_error_preserves_receiver`, which decodes the actual generated CounterRef and invokes it both directly and through BAML (`/tmp/baml-interface-error-shared-python2.log`). The first shared run intercepted the decoder used by ordinary generated calls as well as reporting; the test interceptor now delegates typed ordinary-call decoding and retains only report deliveries.
- Fresh Node build succeeds in 3m37s. Native owned-error, result-transfer, dispatch-queue and release-queue suites pass 34/34 in 10.95s (`/tmp/baml-interface-error-final-node.log`). These validate delivery/ownership; they do not claim generated Node interface methods exist.
- Parity lint exits 1 with the new canonical `spawned_interface_error_preserves_receiver` case present only in Python. Its other nine environments remain missing, as do the two F44 cases. No baseline weakening or skip directives (`/tmp/baml-interface-parity-f45.md`).

Native Python stubs were regenerated and the private GC hook copied into the shipped stub. Scoped Rust/BAML formatting, Python Ruff and diff whitespace checks pass. The BEP/site remain unchanged. All started processes are terminal. No PR/push/CI or snapshot updates were performed in this checkpoint. Next: issuing authority for host-callable argument delivery, recursive input roles and actual interface ports/parity across the remaining SDKs; preserve the full client/Agent/media/alias and ownership gates.

### F46. Carry callback argument authority and the registration's SDK codecs

In progress. The engine now accepts a once-installed bridge context. Native host-call deliveries carry that context with their owned argument aggregate. The CFFI binding stores only a weak engine reference and the original invocation session; Python and Node upgrade that issuer when preparing a delivery rather than selecting the current global runtime. Argument receipt adoption uses a separate session. This does not yet finish queued cancellation/admission, executor routing, scopes, or the remaining C/WASM transports.

Python callable registrations now also capture their selected SDK typemap. Dispatch activates that context through argument decoding, user code, nested registration and success/throw encoding, restoring the previous context on return/unwind. Opaque exception lookup continues to return the original object. Added a shared `visit_counter` fixture and Python callback case that invokes a received interface and retains it past the callback. Native variants replace the default typemap before delivery and test both continued invocation and replaced-runtime rejection.

Initial evidence: native dispatch/completion tests pass 10/10 (4m52s compilation, 0.013s execution). The initial Python probe against the old addon did not finish; interrupted at 290.70s, with no completed test. Rebuild and bounded diagnostic rerun are pending; this is not passing Python evidence. No CI/PR has been created for this checkpoint.

Correction to F45: source inspection found the existing public `baml.sys.collect_garbage()` builtin, explicitly intended for diagnostics/tests. Removed the redundant private Python GC hook and Bex trait forwarding method. Native and shared Python tests now drive that builtin on their bound runtime; the shipped stub no longer declares the private hook. Regenerate the intermediate stub and reverify those tests before closing this checkpoint.

F46 probe findings before final verification:

- Rebuilt Python callback re-entry also hung; interrupted after 54.58s with Python stack diagnostics and `/tmp/baml-interface-callback-hang.sample`. The callback's asyncio loop was running inside an async Tokio task. Dispatch now uses `spawn_blocking`, keeping async workers available for reentrant BAML calls. Application-loop affinity and cancellation of arbitrary blocking Python remain open.
- Registered-map probes reached user code with the correct map but both copied-record success and declared-error outcomes failed strict validation. Sparse inbound classes carry their name in the type annotation; validation was peeling that annotation and checking the anonymous payload. Restore only nominal identity/type arguments before checking fields against the exact declaration. Do not coerce fields before validation: an int must still fail a float callback result. Added an engine case for correct/wrong class identity and correct/wrong field type on both return and throw paths.
- Initial Node rebuild passes (2m18s), and owned-error/result-transfer/dispatch/release tests pass 34/34 in 14.96s. This precedes the sparse-class engine correction, so final validation must use the later build.

F46 verified Python/runtime evidence:

- Rebuilt addon with the dispatch and sparse-class corrections: 1m24s build. The five focused re-entry/issuer/map/collection cases pass in 19.67s (`/tmp/baml-interface-callback-python-probe2.log`).
- Broader Python background-error, runtime-binding, interface-reference and concrete-reference suites pass 59/59 in 216.98s (`/tmp/baml-interface-callback-python-full.log`). The separate callback suite passes 17/17 in 49.02s, now with deterministic collection and no release xfail (`/tmp/baml-interface-callback-callables.log`). These selections overlap the five focused cases; do not sum them as distinct tests.
- Full generated SDK emits 254 files in 6s. Shared Python fixture passes 12/12 in 0.49s, including the new callback's method re-entry and retained receiver (`/tmp/baml-interface-callback-shared-python.log`).
- Native issuer/argument ownership and strict completion checks pass 13/13 in 4.375s after a 5m19s build. The test filter still built 33 binaries. It finished just before the attempted interruption; the targeted rerun (`--lib --test host_value_callable`) also passes the same 13 cases in 4.442s with 0.52s incremental compilation. Use explicit target selection for subsequent iterations. This selection contains two engine cases and eleven native dispatch/completion cases; the runtime-created-name filter matched no test.
- Parity lint exits 1: 724 canonical cases, required gaps 4457→4492 relative to the unchanged baseline. `interfaces/callback_argument_preserves_receiver` is present only in Python and missing in all other nine environments. No weakened requirements, baseline edits or skips (`/tmp/baml-interface-parity-f46.md`).

The generated callback annotation still says `Callable[[CounterRef], int]`, so the async handler's successful execution is not yet a positive static-typing result. Close the recursive input-role/callback awaitable gate with generated positive/negative consumers; do not hide this mismatch with `Any` or casts. Final native Node build/tests, stub refresh and WASM compilation remain to check for this checkpoint.

F46 final bridge checks:

- Combined Node addon/Python stub-generator build passes in 2m51s. Regenerated native stubs and verified the redundant private GC hook is absent from both copies. Directly launching `stub_gen` initially failed because its dependency expects `CARGO_MANIFEST_DIR`; supplying the actual package path, as Cargo does, completed generation. No ABI declaration changed for callable registration's new internal map capture.
- Final Node run: 53/54 pass in 65.76s. Its first callback case hit the unchanged five-second timeout at 5.70s while other suites and WASM compilation were running. That exact case passes alone in 3.85s with the same timeout (4.61s total). Report this as a failed combined run plus a passing isolated retry, not a single 54/54 run. Logs: `/tmp/baml-interface-callback-final-node.log`, `/tmp/baml-interface-callback-node-retry.log`.
- `cargo check -p bridge_cffi --target wasm32-unknown-unknown --no-default-features` passes in 4m17s (`/tmp/baml-interface-callback-wasm-check.log`). This checks the shared issuer/context wiring, not a completed WASM interface delivery port.
- The runtime-created Packet completion test now covers sparse anonymous class payloads as well as named payloads after moving GC, retaining strict rejection of a wrong interface child or int-for-float field. Its targeted run and the JSON completion guard are pending. This addresses the runtime-created-schema case omitted by the earlier misspelled filter.


F46 checkpoint closed with the final targeted runtime-schema/JSON checks passing 2/2 (17.16s incremental compilation, 4.668s execution; `/tmp/baml-interface-callback-runtime-schema.log`). Sparse runtime-created Packet values preserve exact receiver/type contracts across moving GC and still reject invalid children and numeric fields. Rust formatting, Python Ruff and scoped whitespace checks pass. All started processes are terminal. No BEP/site/artifact updates, snapshots, PR/push or CI actions in this checkpoint.

Next implementation work: fix generated callback/input roles and verify positive/negative native typing, then continue the remaining interface ports and shared SDK parity. Keep executor routing, queued admission/cancellation, scopes/cycles, generic evidence, client/Agent overrides, media/PIL and canonical aliases on the main checklist. This checkpoint establishes the native Python callback path and shared issuer plumbing; it does not close the overall implementation goal.

### F47. Directional Python inputs, callback typing and retained callable ownership

In progress. The preceding goal turn made verified implementation progress (F46); no running work is being resumed. Inspected the current generator, codec, and returned-closure implementation.

Use separate input/output translation recursively. A host callback receives output values and produces input values, with supported async completion in its return annotation. A BAML callable receives input values and produces output values. Container inputs should expose read-only covariant views that the encoder actually accepts. Preserve invariant BAML class arguments in their canonical native representation; substituting interface Input types inside an invariant class would invent a different BAML instantiation. Alias and optional-callable protocol emission must keep the same directional distinction. Generic record construction/input views remain an explicit separate gate, not silently widened constructors.

Before claiming those annotations, fix returned BAML closures: they currently fetch the global runtime/map on invocation and are re-registered as host callbacks on inbound encoding. Keep their original invocation owner and codec map, return the original handle when passing back, and give copies/close the same lease semantics as interface refs. Keep their existing synchronous native call behavior; this ownership repair does not need a second registration or a global runtime fallback. Verify actual generated inputs with positive/negative typing and shared SDK runtime cases, preserving the full parity requirement.

F47 implementation and findings:

- Returned Python closures now retain their issuing handle and typemap, invoke through owned call preparation, and pass back through the native carrier encoder. Copies hold separate leases; closing one leaves the others usable. Native probes verify map replacement, runtime replacement rejection, original-handle forwarding without callback registration, and handle cleanup.
- Python input translation now recurses through sequences, mappings, unions, aliases and callables. Callback parameters use output types; callback completions use input types or awaitables. Canonical BAML callable types reverse that direction. Invariant class arguments remain canonical. The encoder accepts `Sequence`/`Mapping`; the native callback dispatcher recognizes custom awaitables as well as coroutines.
- Optional callback Protocols preserve generic parameters and exact keyword names. Initial positive typing failed because open TypedDicts allowed extra keywords. Closed TypedDicts match dispatch and accept ordinary native defaults. Alias companions and helper families use collision-aware allocation. Generic escaped keywords still require a checked-adapter implementation; the current generation diagnostic is an open gate.
- New fixture errors were corrected before claiming generation: callback aliases need explicit `throws`, maps use `values().map(...)`, and abstract interface methods do not have trailing semicolons. Early generator failures came from outdated collection-input expectations, missing newlines between alias companions, and a private helper visibility error. Final results below supersede those failed runs.

F47 verified results:

- Generator unit suite: **131/131**, 0.092s execution (`/tmp/baml-interface-input-generator5.log`). Final CLI build passes in 25.75s; a fresh generation emits 254 files in 5s (`/tmp/baml-interface-input-cli3.log`, `/tmp/baml-interface-input-generate4.log`).
- Strict Pyright 1.1.410: positive consumer has **zero diagnostics**; negative consumer rejects **all fourteen marked calls**, with no additional diagnostics. Includes generic free functions and generic interface methods. Reproducible runner: `sdks/python/.venv/bin/python interface_probes/python/check_input_roles.py <directory-containing-baml_sdk>`. It selects the actual SDK Python interpreter; an earlier invocation without that environment missed native base types and is not validation evidence. Final logs: `/tmp/baml-interface-input-final-positive.json`, `/tmp/baml-interface-input-final-negative.json`.
- Rebuilt Python addon: 16.82s. Native interface-reference and host-callable suites pass **41/41 in 101.04s** (`/tmp/baml-interface-input-native-python.log`). This includes the two earlier focused closure tests; do not add those counts together.
- Final generated shared Python fixture: **20/20 in 0.39s** (`/tmp/baml-interface-input-final-shared.log`). Eight new canonical cases cover retained callable receivers, async aliases, optional keyword arguments, container implementations, nested callbacks, generic callbacks, custom awaitable interface results and generic interface-method callbacks. Earlier 19-case execution preceded the last case.
- Parity lint still exits 1: **732 canonical cases**, required gaps **4457 → 4564** against the unchanged baseline (`/tmp/baml-interface-parity-f47-final.md`). The eight new cases are Python-only, adding 72 missing environment/case pairs compared with F46. No skips or weakened requirements. This must be resolved by actual other-bridge implementations and tests before the one-PR completion gate.

Scoped Rust formatting, Python Ruff and whitespace checks pass. No snapshots, BEP/site/artifact updates, PR/push or CI actions in this checkpoint. All started processes are terminal.

Next: continue host-interface registration and other bridge ports with these same shared cases, retaining the exact type/lifetime contract. Keep generic record inputs, generic escaped keywords, full callable ref APIs, application-loop routing, queued admission/cancellation, scopes/cycles, source-only identities, all-language parity, client overrides/Agent.run, media/PIL and canonical aliases on the main checklist. F47 closes the tested Python directional callback/container path; it does not complete the implementation goal.

### F48. Node references invoke their original issuer

In progress. F47 was verified progress; all its processes are terminal. Node's owned receipts already preserve the engine and admission session, but its native runtime wrapper still looked up the global engine on every call and exposed no interface-method operation. Bind each SDK runtime weakly to its original engine/session, consume argument transfers even when that binding has expired, and prepare handle calls before returning a Promise. Add local lease release without revoking copies or admitted work. Verify actual Node/native interface dispatch, replacement rejection and cleanup before building generated callers on this API. This is a necessary part of the TypeScript port, not proof of generated Node/browser/worker interface support.

Implemented native `_callInterfaceMethod`, `_callOwnedFunction` and its synchronous internal counterpart using common prepared calls and owned result receipts. Handle clones retain the original admission session; local `close()` drops only the wrapper's lease. Runtime wrappers retain a weak issuer instead of fetching the singleton for each invocation. Added `_liveHandleCount` for the same native-table ownership assertions used by Python; this is a test diagnostic, not a new GC API.

Probes use the checked-in shared BAML fixture through actual Node/native entry points. They cover shared receiver state, pass-back, inherited/default dispatch, immediate local close after admission, provisional-result handling, callback re-entry, positional generic method evidence, runtime replacement, and native-table/receipt cleanup. The first run passed nine cases and failed the generic case because the probe sent a named type argument; methods require positional evidence. Corrected the probe instead of weakening the native contract.

Review found that Node's encoder rolled back host registrations but leaked native keys cloned before a later encode failure. The same transactional rollback now handles ordinary arguments and callback success/throw payloads. The added callback-failure tests then observed one temporary exception transport owner per failure. On same-host exception rehydration, the original JS exception is the output; the decoder now releases its discarded transport wrapper during receipt adoption. Final verification must include exception identity and result-transfer tests as well as the new cases.

F48 verified results:

- Native `cargo build -p bridge_typescript` passes in **48.14s**. The packaged NAPI build, run from the workspace root with explicit manifest/package paths, passes in **2m14s** (`/tmp/baml-interface-node-owner-build.log`, `/tmp/baml-interface-node-owner-napi.log`). NAPI generated the loader and declarations; copied its declarations into the TypeScript source tree. Its explicit host target uses a different Cargo artifact directory from the initial build; keep that distinction when reusing builds.
- Package TypeScript compilation passes, and strict standalone checking of `tests/interface_ownership.test.ts` passes (`/tmp/baml-interface-node-owner-tsc3.log`, `/tmp/baml-interface-node-owner-test-types.log`). This checks the native test surface, not generated interface typing.
- Final interface ownership, result-transfer, host-callable and owned-spawn suites pass **55/55 in 49.60s** (`/tmp/baml-interface-node-owner-verified.log`). The thirteen new ownership cases assert that both native table leases and pending receipts return to baseline after each case, including callback result/throw encoding failures and same-host exception identity. Before the final error-wrapper cleanup, the expanded suite failed six assertions due to the two temporary transport owners; that failed run is not passing evidence.
- Media wrappers pass **18/18** in the earlier combined run (`/tmp/baml-interface-node-owner-final.log`, 71 total passing cases before the two extra callback-failure cases and their decoder correction). Keep that earlier result separate from the final 55-case run; do not present a fabricated combined total.
- Scoped Rust formatting and diff whitespace checks pass. The shared SDK declarations did not change in F48, so the F47 parity failure remains: 732 canonical cases and 4564 missing required environment/case pairs. No baseline edits, skips, snapshots, PR/push, CI, BEP/site or artifact updates. All started processes are terminal.

F48 native-layer checkpoint complete. Next, make TypeScript generated/runtime callers capture SDK runtime and codec context, decode interface/concrete references into callable wrappers, and forward returned callable handles through their owner operations. Current `decodeBamlClosure` still uses the global runtime and re-registers on pass-back; that is an explicit next repair. Then generate exact associated/generic caller/input roles and execute the existing shared SDK cases on Node, browser and workers. Retain the remaining host-interface binding, other-language, scope/cycle, client/Agent, media and alias gates.

### F49. Capture TypeScript SDK codecs and generated function context

In progress. F48 was verified progress; its processes are terminal. Encoding/decoding now accept the SDK's typemap explicitly, including nested native type tokens and callback arguments, asynchronous results and declared throws. Returned Node closures retain that map and invoke their owned native handle; pass-back forwards that handle without registering another host callback. Preserve the existing synchronous callable surface while the general callable API remains unfinished.

Function factories capture a runtime/map pair; generated factories use a lazy getter for their own SDK pair so ESM import cycles can define functions before setup completes. Generation now emits `_sdk.ts` with hoisted factory wrappers, shared initialization and the captured pair. Root and nested leaves import those wrappers. Native/public generated interface classes and browser/worker issuer support are still separate pending ports. Verify real generated imports and runtime replacement, not just string assertions.

F49 verified results:

- Shared TypeScript generator suite passes **48/48**, 0.037s execution (`/tmp/baml-interface-node-context-generator2.log`). The initial run had 43 passes and five assertions expecting setup/imports in the former locations; assertions now check `_sdk.ts` and per-leaf wrapper imports. No snapshot changes.
- Package TypeScript compilation and strict native-probe checking pass (`/tmp/baml-interface-node-context-tsc.log`, `/tmp/baml-interface-node-context-test-types.log`). The initial codec compile caught an accidentally added unused typemap parameter on the union-unwrapping helper; removed it before running probes.
- Final SDK-context, native interface-ownership, host-callable, typemap and result-decoder suites pass **52/52 in 48.75s** (`/tmp/baml-interface-node-context-native.log`). Six new context cases exercise copied-record constructor identity, callable pass-back through the sync guard, async completion, declared callback throws, nested type tokens and replacement rejection. The earlier context/result-transfer selection passes 24/24 in 6.07s; it overlaps the final selection and is not an additional distinct-test count (`/tmp/baml-interface-node-context-probe.log`).
- CLI build passes in **26.76s**; full shared fixture plus `interface_probes/baml/sdk_context.baml` generates **68 TypeScript files in 8s** (`/tmp/baml-interface-node-context-cli.log`, `/tmp/baml-interface-node-context-generate.log`).
- The actual generated consumer passes strict TypeScript compilation and executes to completion, including `baml.identity_async` with an explicit generated class token after changing the default typemap (`/tmp/baml-interface-node-context-generated-types2.log`, `/tmp/baml-interface-node-context-generated-runtime2.log`). Both root and nested calls reject the replacement runtime. See `interface_probes/typescript/README.md` for reproduction. This is a copied-record/context probe; generated interfaces still translate to `unknown` and have no caller API.
- Scoped Rust formatting and whitespace checks pass. All started processes are terminal. No shared SDK test declarations changed, so the F47 parity failure remains unchanged; no baseline edits/skips, PR/push, CI, BEP/site or artifact updates.

F49 context checkpoint complete. Next: generate and decode TypeScript interface/concrete reference wrappers, preserve associated/generic input and caller roles, and execute the shared interface tests through those generated APIs. Carry the same ownership changes into browser/worker transports; their current native API is not yet compatible with the new owned callable operations. Keep full callable async/lifecycle semantics, context propagation through streams/introspection/direct helpers, host implementations, scopes/cycles, other languages, client overrides/Agent, media/PIL and canonical aliases on the main checklist.

### F50. Generated TypeScript interface callers

In progress. Replace interface-to-unknown projection with generated retained Ref callers and lazy decoder entries. Reuse the semantic associated-type substitution from `baml_codegen_types` (extracted from Python) and the compiler's resolved inherited caller surface. Preserve interface identity and invariant associated pins, including unused Error pins. Keep concrete-Self, receiverless and missing-required-pin methods out of an unqualified existential caller.

Verification must include actual generated strict TypeScript consumers and execution against the Node addon, not only native handle tests. Those native tests now need explicit harness Ref codec entries because interface decoding no longer silently returns an untyped handle. Input witnesses, concrete facade behavior, generic type evidence and checked specialization remain necessary gates; do not mistake a completed Ref renderer for the whole TypeScript port. Browser/worker native operations remain pending. No BEP/site edits.

F50 initial generated verification: the Node native interface/context/result-transfer selection passes 37/37. Strict generated caller checks pass, with eight expected negative sites. Generated runtime assertions cover associated media decoding, generic Echo, non-class Iterable/Iterator, nested live fields, inherited views, local-close/admitted-call behavior, callbacks and copied marker records. This initial success output did not establish natural process exit; the later lifetime probe below found and fixed a hang. Shared suite initially passes 13/14; the remaining failure exposes a bare `BamlHandle` from the concrete FriendlyGreeter factory. Updated that case to the actual `new_async` signature and require the generated concrete class plus direct methods.

Closing that gap in this checkpoint: reuse owned-function admission with a concrete-method target; the compiler supplies each method's selected signature and interface pattern. Emit live concrete facades without copied field constructors, preserve static factories, and decode `CONCRETE_OBJECT` through the issuing SDK's class map. Common ref ownership serves both concrete and interface wrappers. This uses existing native operations; it does not claim host implementation registration or WASM support. The TypeScript generator now consumes the common wire type encoder for concrete interface patterns, adding the same bridge_ctypes/prost dependency used by Python; extracting a small pure wire encoder remains a build-cost follow-up.

F50 completed caller implementation:

- Shared semantic projection preserves original parameter slots and associated declaration order. TypeScript generates inherited interface callers, exact input witnesses (including multiple views of one interface), recursive input containers/aliases/callbacks and concrete methods from the compiler's selected contracts. Required-interface refs carry the required input/caller evidence; a redundant per-ref private brand was removed because it prevented valid required-interface assignment.
- Live wrappers are frozen, reject JSON serialization and retain their original runtime/map. Clones and admitted calls survive local close. Runtime-owned classes preserve their existing constructor/codec identity. Lifecycle names, JavaScript `then`, generated helpers and private generic variables are allocated against authored names; positive/negative probes cover the new collision rules.
- Actual offline factories return `ResponsesClient` and `Agent`; concrete generic methods retain precise records, type arguments and concrete Self. Full overrides/Agent execution and streaming remain open.

F50 process-lifetime finding and correction:

- The interface consumer printed success but never exited; explicitly collecting BAML garbage made it exit. Callback registrations held referenced Node delivery channels until VM GC. Stopped the three obsolete hung probe processes; they were not passing executions.
- Callback delivery now retains the callable without keeping the Node event loop alive. Before automatic shutdown, the SDK awaits an observational engine idle operation with admission open, allowing background callbacks to re-enter BAML. A weak registry includes retiring runtimes. Shutdown then performs its ordinary final drain. This is not a promise of immediate collection in a running singleton or worker isolation.
- Added six bounded child-process tests using the public SDK, with no manual shutdown, GC, forced exit or JS keepalive timer. The first run failed fixture compilation because the returned function type omitted its explicit throws contract; corrected that fixture before assessing the lifetime change.

F50 verified results:

- Shared Python/TypeScript generator suites: **179/179**, 33.45s build and 0.208s execution (`/tmp/baml-interface-ts-final-generators2.log`). Final CLI builds in 51.55s and emits 69 files in 9s (`/tmp/baml-interface-ts-final-cli2.log`, `/tmp/baml-interface-ts-final-generate2.log`). The later required-ref private-brand removal is covered by actual strict generated checking and runtime execution.
- Strict generated TypeScript consumers pass positive checking and **all thirteen expected rejection sites**, including an Error-only mismatch, undeclared structural implementations, missing method type evidence, unpinned requirements, live fields/constructors and conflicting input views (`/tmp/baml-interface-ts-final-generated-types.log`). This does not statically connect the current unparameterized BamlTypeToken to its native generic T; that remains a gate.
- Final lifetime addon build: **1m33s**; regenerated native declarations and package TypeScript compile successfully (`/tmp/baml-interface-ts-idle-build.log`, `/tmp/baml-interface-ts-idle-types.log`). Strict checking of the new subprocess test also passes (`/tmp/baml-interface-ts-idle-test-types.log`).
- Process lifetime: **6/6**, 23.28s total (`/tmp/baml-interface-ts-idle-probe2.log`). Three generated consumers exit **0** under a 25-second subprocess bound without explicit GC/shutdown: interfaces, multiple input views and concrete methods (`/tmp/baml-interface-ts-idle-generated_interfaces.log`, `/tmp/baml-interface-ts-idle-generated_interface_inputs.log`, `/tmp/baml-interface-ts-idle-generated_concrete_methods.log`).
- Final serial native selection: **73/73**, 98.24s total (`/tmp/baml-interface-ts-idle-native-serial.log`). Command: `node_modules/.bin/vitest run --no-file-parallelism tests/dispatch_queue.test.ts tests/release_queue.test.ts tests/interface_ownership.test.ts tests/sdk_context.test.ts tests/result_transfers.test.ts tests/owned_spawn.test.ts tests/host_callable.test.ts` from the Node bridge package. Its parallel predecessor was **69 passes / 4 failures**, with timeouts and subsequent cleanup assertions (`/tmp/baml-interface-ts-idle-native.log`); the serial run passes without raising timeouts or changing assertions. Earlier 37-case selections overlap these tests.
- Final shared generated Node fixture: **16/16**, 66ms execution / 1.46s total (`/tmp/baml-interface-ts-idle-shared.log`). Nine added canonical cases reuse Python IDs. The shared TS source also contributes declarations for two web environments; only Node execution is verified.
- Unchanged parity ratchet still exits **1**: **732 canonical IDs, 4537 required gaps**, against baseline 4457; 2175 declarations versus baseline 2069; **zero weakened requirements** (`/tmp/baml-interface-parity-f50.md`). This reduces F47's 4564 gaps by 27 declaration/environment pairs but does not prove web execution. No skips or baseline changes.

Final shared-engine check: `cargo nextest run -p bex_engine --test fire_and_forget -E 'test(shutdown_waits_for_active_calls_and_rejects_new_calls) | test(cancelled_shutdown_restores_the_running_state) | test(call_completion_does_not_join_but_shutdown_does)'` passes **3/3**, 1m09s build / 5.655s execution (`/tmp/baml-interface-ts-idle-engine.log`). The 24 deselected tests are outside this narrow filter, not newly skipped requirements. Scoped Rust formatting and diff whitespace checks pass.

F50 caller/lifetime checkpoint complete. The next implementation gate is checked TypeScript interface specialization and typed method evidence, followed by the remaining host implementations, callable lifecycle/scopes, ports and shared execution. Preserve the broader all-language, client/Agent, media and alias acceptance conditions. This does not complete the overall implementation goal. All started processes are terminal. No BEP/site/artifact edits, snapshots, PR/push or CI actions in this checkpoint.

### F51. Checked TypeScript interface projection and explicit type evidence

In progress. F50 made verified implementation progress; its started processes are terminal. Use the existing owned `baml.identity` path, as Python does, to obtain a fully specified interface view on the same receiver. Generate `Ref.type(...)` with ordinary arguments followed by associated bindings in declaration order, and expose `value.as_interface(target)`. The target retains its exact generated constructor identity; the receiving SDK checks that identity recursively through composed type evidence before native admission. Native conformance checks remain authoritative, including wrong Output/Error choices and runtime replacement.

`BamlType<T>` now carries invariant native evidence. Primitive factories and typed array/optional composition preserve T. An erased `BamlTypeValue` remains available at wire/reflection boundaries; a dynamic reflected type is not promoted to statically known string evidence. General generated class/enum type constructors and native method evidence still need their full treatment. This checkpoint adds the mechanism needed for an exact projected caller, not an arbitrary cast from a reflected schema.

New finding: `BamlTyDef.witnesses` belongs to its root class. Wrapping that root in an array, optional or interface argument would change the witness meaning. Reject that embedding explicitly until the shared graph protocol can carry nested witness owners. Preserve/deduplicate class and enum definition tables and reject conflicting definitions; never silently discard conformance metadata. Record the final API and probe results in the adjustment log after verification.

Ported the existing Python canonical cases `checked_interface_projection_preserves_receiver` and `checked_interface_projection_survives_reference_release` into TypeScript. Added `checked_interface_projection_rejects_wrong_bindings` in both languages, including the media decoder's associated bindings. An initially duplicated Python test declaration was removed; the final source has no duplicate function definitions. The generated Node probe additionally covers an unpinned required interface, a record result, exact native typing, Error-only mismatch, unregistered facade subclasses and nested SDK identity.

F51 implementation findings:

- A typed private field did not preserve invariance in the runtime package's `.d.ts`; TypeScript removes its annotation. Replaced it with internal symbol-keyed evidence. Concrete refs now likewise retain a literal declaration name and exact generic pins through a symbol on their generic base. The final probe rejects widening `FacadeBox<string>` to `FacadeBox<unknown>` against emitted declarations as well as source.
- Ordinary generated record constructor tokens retain exact SDK constructor checks through composition. Token construction currently resolves such a constructor in the active map; generated class type factories must remove that remaining multi-SDK construction dependency and supply exact generic arguments. Primitive and interface selectors do not perform that global lookup. A private construction key prevents ordinary subclasses or raw JavaScript construction from claiming a native T for an unchecked schema; internal codegen hooks remain internal.
- The Python wrong-binding probe initially expected a BamlError. The actual existing invalid-input channel is TypeError; corrected the assertion to require the conformance diagnostic. The receiver remains usable. A native Node test also verifies rollback when type evidence fails after an argument's receiver lease was encoded.

F51 verified results:

- Final TypeScript generator suite: **48/48** (`/tmp/baml-interface-ts-projection-generator3.log`). CLI build passes in **26.28s**; final generation emits **69 files from seven BAML files in 3s** (`/tmp/baml-interface-ts-projection-cli3.log`, `/tmp/baml-interface-ts-projection-final-generate.log`). Earlier generator/CLI runs caught an unqualified Rust helper name; fixed it before these final runs.
- Package TypeScript and strict native test typing pass. Real generated consumers pass against both generated source and separately emitted SDK `.d.ts` files, with **all 21 expected rejection sites**. Reproduction: `python3 interface_probes/typescript/check_generated_declarations.py /tmp/baml-node-interface-context-review`. Logs: `/tmp/baml-interface-ts-projection-final-build-types4.log`, `/tmp/baml-interface-ts-projection-final-native-types.log`, `/tmp/baml-interface-ts-projection-final-declarations.log`, `/tmp/baml-interface-ts-projection-final-compiled-types.log`.
- Four real generated consumers exit **0**, each under a 25-second process bound: projection, interface calls, multiple input views and concrete calls (`/tmp/baml-interface-ts-projection-verified-generated_interface_projection.log`, `/tmp/baml-interface-ts-projection-verified-generated_interfaces.log`, `/tmp/baml-interface-ts-projection-verified-generated_interface_inputs.log`, `/tmp/baml-interface-ts-projection-verified-generated_concrete_methods.log`). Includes named-record results after changing the default map, nested SDK identity rejection and natural exit.
- Native ownership/type-evidence/context/typemap/result-decoder selection passes **38/38 in 11.19s** (`/tmp/baml-interface-ts-projection-final-native2.log`). The new tests cover conflicting definition tables, root-scoped witness rejection, failed-evidence lease rollback and construction guards. Earlier 19-, 37- and four-case selections overlap and are not additional distinct totals.
- Shared Python projection selection passes **3/3 in 1.47s**, with 18 other cases deselected (`/tmp/baml-interface-projection-python-shared3.log`). Shared generated Node suite passes **19/19**; final result is in `/tmp/baml-interface-ts-projection-verified-shared-node.log`. Only Node, not the two web environments sharing those declarations, was executed.
- Parity lint exits **1**: **733 canonical IDs, 4537 required gaps**, baseline 4457; 2185 present declarations versus baseline 2069; **zero weakened requirements** (`/tmp/baml-interface-parity-f51.md`). Two ports close six environment/declaration gaps; the new case adds six unmet environments, leaving the total unchanged from F50. No skips or baseline changes.

F51 checked-projection checkpoint complete. Source BAML formatting, scoped Rust formatting, Python Ruff and scoped diff whitespace checks pass. All started processes are terminal. Remaining work includes full typed method/function evidence, generic/enum type construction, root-witness graph composition, dynamic schema codecs, host implementations and all remaining language/lifetime/client/media/alias gates. This checkpoint does not complete the overall goal. BEP/site/artifact unchanged; no snapshots, PR/push or CI actions.


### F52 — Typed TypeScript method choices and nominal type factories (verified slice)

- [x] Tie every interface/live-concrete method type slot to invariant `BamlType<T>`, with inference driven by tokens. Use allocated `BamlNoInfer`/`BamlType` imports so authored helper names cannot shadow these relationships.
- [x] Generate `FooType(...)` companions for nominal classes/enums, including nested generic arguments and collision suffixes. Capture the issuing SDK map; exclude builtin carrier declarations through `compiler_aliases`.
- [x] Compose type graphs once for nominal/interface factories, preserving definitions, nested SDK checks and conflict rejection. Continue rejecting embedded root-scoped witnesses until their ownership is represented in the protocol.
- [x] Exercise source and emitted declaration consumers, real Node execution, and matching shared Python/TypeScript generic-method success/rejection cases.
- [x] Resolve the discovered TypeScript enum input gap through exact contextual enum reconstruction, checked variant membership and retained union ambiguity handling.

The generated method has the relationship `echo<T>(value: NoInfer<T>, opts: { $types: { T: BamlType<T> } }): Promise<T>`. Raw method tokens no longer serve as unchecked evidence. A string token returns `string` rather than the caller's input literal type. Tests reject inconsistent explicit native parameters, wrong values/callback results, missing class arguments and fabricated nominal type changes.

The first real enum probe failed despite successful static checking: TypeScript emits enum members as strings. The engine now uses the checked enum context to reconstruct a variant and verifies it against the retained declaration. A contradictory explicit string annotation still fails. Enum arrays and nullable enum inputs work; strict overlapping `enum | string` inputs require a selected alternative. This shared conversion rule parallels contextual record decoding; it is not structural interface implementation by strings. The internal [adjustment log](INTERFACES_DESIGN_ADJUSTMENTS.md#f52--method-generics-need-typed-evidence-enum-values-need-contextual-decoding) records the reasoning.

Verification (all Cargo commands from `baml_language/`):

| Check | Result / evidence |
|---|---|
| `cargo nextest run -p sdkgen_typescript_shared` | **48/48**, `/tmp/baml-f52-generator.log`. |
| `cargo build -p baml_cli`; regenerate probe projects | CLI **46.94s**. Node **69 files / 7 inputs / 8s**; Python **254 files / 2 inputs / 17s**. Logs `/tmp/baml-f52-cli.log`, `/tmp/baml-f52-shared-node-generate.log`, `/tmp/baml-f52-shared-python-generate.log`. |
| Runtime TypeScript compile and native-test type checking | Pass (`tsc` in Node package; `tsc --strict --noEmit` for both native test files); `/tmp/baml-f52-test-types.log`. |
| `python3 interface_probes/typescript/check_generated_declarations.py /tmp/baml-node-interface-context-review` | Source and emitted declarations pass all positive checks and **31** expected rejections. `/tmp/baml-f52-verified-declarations.log`. |
| Compiled Node consumers | **4/4 exit 0** under 25-second process timeouts: interfaces, inputs, concrete methods and declaring-view projection. `/tmp/baml-f52-verified-generated_*.log`; compile log `/tmp/baml-f52-verified-consumer-types.log`. |
| `cargo nextest run -p bex_engine --test interface_handles` | **6/6**, including erased enum members, nullable/ambiguous enum inputs, bad variants and contradictory source annotations. `/tmp/baml-f52-engine-final2.log`. |
| `cargo nextest run -p bex_engine --lib -E 'test(conversion::)'` | **83/83**, `/tmp/baml-f52-conversion-final.log`. |
| `cargo build -p bridge_typescript`; install built dylib as packaged Node addon | **1m20s** including artifact-lock wait. Copied `target/debug/libbaml_node.dylib` to `sdks/typescript/bridge_typescript/dist/baml_node.darwin-arm64.node`; `/tmp/baml-f52-addon-final2.log`. No exported native API changed, so the existing generated loader/declarations remain applicable. |
| Vitest `tests/type_evidence.test.ts tests/interface_ownership.test.ts --no-file-parallelism` | **21/21**, **5.53s**, against the rebuilt addon. `/tmp/baml-f52-verified-native.log`. |
| Shared Node Vitest fixture | **21/21**, **1.15s**. `/tmp/baml-f52-shared-node.log`. |
| Shared Python fixture, regenerated SDK | **23/23** using the existing installed Python native bridge. `/tmp/baml-f52-shared-python-all.log`. Python carries explicit enum variants, so these tests establish the common API assertions, not execution of the newly changed Rust conversion in the Python addon. That conversion is exercised by rebuilt Node and direct engine tests above. |
| Parity ratchet | Still fails: **735 IDs, 4549 required gaps, 2193 declarations, 0 weakened requirements**. Two new canonical cases have six missing environment pairs each. `/tmp/baml-interface-parity-f52.md`, `/tmp/baml-f52-parity.log`. |
| Format and whitespace | Scoped Rustfmt, Ruff, BAML fmt and `git diff --check` pass. |

The initial enum roundtrip failure, Python exception expectation (`TypeError`, not `BamlError`), an ambiguous Rust string comparison, and a zero-match conversion-test filter were corrected. The final logs above supersede those attempts; the zero-test command is not counted as verification. No test/build exceeded four minutes, so no CI fallback or PR was used in this slice. No snapshots were changed by F52.

Remaining gates: free-function optional/inferred `$types` and literal inference; generic copied-record `$types` full definition encoding; input-role mapping for generic interface arguments; static generic bounds; canonical builtin type factory surface; dynamic/nested conformance ownership; host implementation registration; browser/worker operations and every other native SDK port. Full clients/Agent execution, media/PIL and all-language parity remain open. This is not completion of the overall goal or any full checkpoint. The BEP, website and interactive artifact are unchanged.


### F53 — Retain generic contracts before registering host implementations (verified prerequisite)

- [x] Preserve complete runtime generic names and bounds on required/default interface method declarations, using the same lowering as function metadata. Keep unused and unbounded slots.
- [x] Include all bound declaration references, arguments and associated pins in linking, tracing and moving-GC fixup.
- [x] Check declaration arity/bounds before implementation dispatch. An engine test strips a provided implementation's bound to prove that the interface declaration independently rejects an invalid choice.
- [x] Advance serialized artifact format from 5 to 6 and rebuild/regenerate actual SDK consumers together.
- [x] Extend the existing shared generic success/rejection cases with `check<T extends RecordMarker, U>` where neither type parameter appears in value arguments/results. No new canonical test names or skipped requirements.

This enables sound registration validation later. It does **not** register a Python/JS object, generate an implementation adapter, or deliver `GenericCall` to a host handler. The [adjustment log](INTERFACES_DESIGN_ADJUSTMENTS.md#f53--host-registration-must-retain-method-quantification) explains the metadata gap and remaining registration decisions.

| Check | Result / evidence |
|---|---|
| Compiler interface declaration metadata | `cargo nextest run -p baml_compiler2_emit --lib -E 'test(interface_def_)'`: **4/4**; `/tmp/baml-f53-descriptor-final2.log`. |
| Checked engine interface entry | `cargo nextest run -p bex_engine --test interface_handles`: **7/7**; `/tmp/baml-f53-interface-final.log`. Includes valid unused arguments, missing arguments and a violated declaration bound despite removed implementation metadata. |
| Bound metadata through moving GC | `cargo nextest run -p bex_heap --test generational -E 'test(interface_owner_and_default_bodies_are_traced_and_forwarded)'`: **1/1**; `/tmp/baml-f53-gc.log`. |
| Artifact encoding/version rejection | `cargo nextest run -p baml_artifact --lib`: **5/5**; `/tmp/baml-f53-artifact-tests.log`. |
| Combined CLI/Python/Node build | `cargo build -p bridge_python -p bridge_typescript -p baml_cli`: **6m49s**, successful; `/tmp/baml-f53-build.log`. The build crossed four minutes and continued making progress; it completed locally. No CI fallback or PR was used. |
| Python extension packaging | Rebuilt with `PYO3_BUILD_EXTENSION_MODULE=1` and `PYO3_PYTHON` pointing to the SDK venv: **2m11s**; `/tmp/baml-f53-python-extension.log`. Verified no `libpython` link before installing the extension. The ordinary combined-build dylib was not installed into the differently versioned Python venv. |
| Matching SDK generation | Python **254 files / 2 inputs / 5s**; Node **69 files / 7 inputs / 4s**. `/tmp/baml-f53-python-generate2.log`, `/tmp/baml-f53-node-generate2.log`. Both native addons installed by atomic replacement. |
| Shared native execution | Python **23/23 in 0.54s**; Node **21/21 in 1.19s**, against rebuilt bridges and regenerated SDKs. `/tmp/baml-f53-shared-python.log`, `/tmp/baml-f53-shared-node.log`. |
| Generated TypeScript source and emitted declarations | Positive consumers and **31** expected rejection sites pass; `/tmp/baml-f53-declarations.log`. Strict consumer compilation also passes; `/tmp/baml-f53-consumer-types-final.log`. |
| Real generated Node programs | **4/4 exit 0**, each under a 25-second bound: interfaces, inputs, concrete methods and declaring-view projection. `/tmp/baml-f53-verified-generated_*.log`. |
| Native Node ownership/type evidence | **21/21 in 5.57s**, `/tmp/baml-f53-verified-native.log`. |
| SDK parity | Still fails with **735 IDs, 4549 required gaps, 2193 declarations, 0 weakened requirements**; unchanged from F52. `/tmp/baml-interface-parity-f53.md`, `/tmp/baml-f53-parity.log`. No baseline changes. |
| Formatting | Scoped Rustfmt, Python Ruff, BAML formatting and `git diff --check` pass. |

The first descriptor test incorrectly assumed an unlinked head had a readable declaration name; it now compares declaration identity. Initial SDK generation referenced a nonexistent `Marker`; corrected the fixture to its existing `RecordMarker`. The final logs above supersede those failed attempts. No snapshots were changed in this slice.

All started checks are terminal. Full host registration, client/Agent execution, media/alias work, lifetime gates and remaining language ports are still open. No full checkpoint or overall goal is complete. The BEP, website and interactive artifact remain unchanged.


### F54 — Check a registration before exposing its implementation rules

The shared registration path now checks coherence in a private proposed world. This fixes a concrete problem in reflected class creation and establishes the mechanism host adapters need: a staged `Gate` implementation can activate a static blanket `Pick` implementation, making a staged explicit `Pick` conflict. Checking only the old table, or registering rows one at a time, misses that case. Associated outputs cannot disambiguate two implementations with the same interface inputs.

- [x] Add a private staged-rule overlay to the existing resolver, including recursive bound proofs; do not insert provisional rows into the live index.
- [x] Validate every registration template before solving any goal. All dynamic receiver types, interface inputs and associated outputs must be realized; unresolved templates must not reach infallible compiler-rule substitution.
- [x] Count existing rule identities once when multiple package paths reach them. Use the resolver's semantic type comparison for input choices.
- [x] Prepare reflected class witnesses before publication. For recursive builders, reject before recording resolved types or freezing connected builders.
- [x] Publish accepted rows in one write lock and preserve the weak index. Correction from F55: the old GC tests explicitly rooted both class and rule; they did not prove class-owned rule tracing. F55 adds the missing ownership edge and tests it.
- [x] Add Rust checks for private-table state and BAML corpus tests for public registration/retry behavior.

This slice changes the shared dynamic registration machinery, not the generated host implementation API. Exact required-interface instantiation validation, host method contracts/trampolines and SDK binders remain open. The [F54 adjustment](INTERFACES_DESIGN_ADJUSTMENTS.md#f54--validate-registration-in-a-private-proposed-world) records the reasoning.

Verification so far: the final focused VM selection passes **7/7** (`/tmp/baml-f54-registration-final.log`): five registration cases and the two existing dynamic-table GC forwarding/sweep cases. The first public BAML rerun passes **4/4 in 66s** (`/tmp/baml-f54-baml-tests2.log`). The initial test source incorrectly used `string.contains`; corrected it to the documented `string.includes`. No runtime failure was hidden by that correction. Final rebuilt-CLI execution and snapshot checks also pass, as recorded below.

The snapshot accept pass succeeds (**one corpus snapshot test**, **13.479s** after **3m41s** build/lock time; `/tmp/baml-f54-snapshots.log`). It adds the new namespace's bytecode snapshot and refreshes root registration plus stale stdlib `baml.identity` PPIR/MIR/bytecode snapshots from earlier implementation. Source review confirms that helper already exists in the worktree. Five snapshot files changed relative to the pre-run hashes; this is not a claim that all five originated in F54. No snapshot contents were manually patched.

No SDK test IDs, skips or parity baseline were changed in this slice. The F53 parity result remains the last executed parity check, not a new F54 pass. No bridge-specific API changed, and no SDK addon was rebuilt in F54. Keep full native SDK/lifetime/client/media/alias gates open. BEP/site/artifact unchanged; no PR, push or CI actions.


F54 final verification:

| Check | Result / evidence |
|---|---|
| `cargo nextest run -p bex_vm --lib -E 'test(registration_tests::) \| test(package_load::tests::)'` | **7/7**, 5.906s after 1m11s build/lock time; `/tmp/baml-f54-registration-final.log`. The earlier 4/4 selection overlaps and is not an additional total. |
| Final `cargo build -p baml_cli` | Pass, **1m07s**; `/tmp/baml-f54-cli-final.log`. Supersedes the initial 2m09s build after the added template preflight guard. |
| `target/debug/baml-cli test --from crates/baml_tests/baml_src -i dynamic_registration` | **4/4**, **67s** including corpus discovery, against the final CLI; `/tmp/baml-f54-baml-final.log`. This selects four runtime tests, not the entire runtime corpus. |
| `cargo insta test --test-runner nextest --dnd --check -p baml_tests --lib -- corpus_snapshots` | **1/1**, **12.319s** after **1m09s** build/lock time, no snapshots to review; `/tmp/baml-f54-snapshots-check.log`. |
| Formatting/whitespace | Scoped Rustfmt check, BAML formatter and `git diff --check` pass. No `.snap.new` files remain. |

All started checks are terminal. No local command exceeded four minutes in this slice. The registration-coherence work is verified, but no full implementation checkpoint or overall goal is complete. The BEP, website and interactive artifact remain unchanged.


### F55 — Ordinary host method bodies and actual heap ownership

- [x] Create ordinary VM method functions over per-instance retained host callbacks. Realize each invocation's parameter/result/error contract from the declaration's complete generic frame; preserve defaults and bound calls.
- [x] Preserve runtime-resolved callable constants during bytecode compaction and give runtime functions a package owner for GC tracing/fixup.
- [x] Connect fresh reflected classes to their implementation rules through a private package. Deduplicate rules found through both the owner and weak index.
- [x] Add receiver/class-only GC checks, including collection during a suspended callback; verify callbacks and weak rows release when their actual owners disappear. Focused VM checks pass 12/12.
- [x] Verify real engine outbound dispatch, completion validation and callback release while the engine remains alive.
- [x] Finish final crate/public-reflection checks and scoped formatting.

The optional-slot test constructs runtime metadata explicitly because the current source grammar rejects defaults and `?:` on interface declarations. It is not a public syntax test. Full host registration/conformance, receiverless methods, advanced GenericCall frame transport and all SDK binders remain pending. No new shared SDK IDs have been added for an API that is not available yet.


F55 final verification:

| Check | Result / evidence |
|---|---|
| `cargo nextest run -p bex_engine -p bex_vm --lib -E 'test(host_interface_tests::) \| test(host_interface::tests::) \| test(registration_tests::) \| test(package_load::tests::)'` | **13/13**, 10.855s after 5m42s build/lock time; `/tmp/baml-f55-runtime-final.log`. Includes the earlier 12 VM checks, not an additional independent total. |
| `cargo nextest run -p bex_vm_types --lib` | **51/51**, 0.067s after 2m47s build/lock time; `/tmp/baml-f55-vm-types-tests.log`. The first narrower `bytecode::tests::` selection matched zero tests; the actual module is `bytecode::compact_tests`. The full crate run supersedes that failed selection. |
| `cargo build -p baml_cli` | Pass, 5m37s including build-lock waiting; `/tmp/baml-f55-cli-build.log`. |
| `target/debug/baml-cli test --from crates/baml_tests/baml_src -i dynamic_registration` | **4/4**, 89s including discovery, against the rebuilt CLI; `/tmp/baml-f55-public-registration.log`. |
| Formatting/whitespace | Scoped Rustfmt and `git diff --check` pass. No `.snap.new` files found. |

The engine test uses the real native outbound dispatcher with primitive wire arguments and in-process external-value completion. It covers a successful generic/default/bound call sequence, an associated output, wrong output and forbidden throw, plus GC before invocation and callback release after the last receiver owner disappears while the engine is still alive. It deliberately assembles a valid adapter below the future registration API. This is not SDK binder or C completion-decoder coverage.

Initial compile failures were a Rust assertion type ambiguity and a test wire-string conversion; both were corrected before the final passing run. F55's earlier runtime probe also exposed the real compact-constant bug described in the adjustment log. No failure was converted to a skip. The longer local commands were rebuilds/lock waits; once their final crates were compiling, completing them locally was useful, so no CI fallback was initiated. No PR, push or CI action occurred.

No public BAML corpus source, snapshot, SDK ID or parity baseline changed in F55. Artifact format remains 6; no new opcode or serialized field was introduced. F54 remains the last corpus snapshot check and F53 the last SDK parity run. No native SDK addon was rebuilt or executed in this slice.

All started checks are terminal. The next concrete step is immutable host adapter registration with complete method, associated-bound and required-interface checks, using the staged resolver and this owned function graph. Advanced frame delivery, receiverless methods, optional interface source syntax, binders and cross-bridge execution remain open. No full implementation checkpoint or overall goal is complete. The BEP, website and interactive artifact remain unchanged.


### F56 — Declaration predicates in the private registration world

The next shared gate checks all generic-argument bounds, associated-type bounds and required-interface instantiations before publication. The compiler now emits registration predicates over `[Self, interface arguments...]`, including associated projections and only written constraint pins. Runtime validation uses the same private staged resolver for projection and membership; consulting BexVm's published-world projection would miss staged bindings.

- [x] Add compiler-owned predicate metadata and include its heads in linking/tracing/forwarding.
- [x] Add exact associated-binding completeness and declaration-constraint validation after full template/coherence preflight.
- [x] Route reflected class and builder preparation through that validator, replacing the earlier head-only requirement check; preserve the correct diagnostic category.
- [x] Verify compiler serialization, staged constraints, GC and public reflection behavior.
- [x] Finish design updates and final checks.

Artifact format is now 7 because compiled interface metadata gained predicates. Rebuild native runtimes and regenerate SDK bytecode together. This is not a new bridge registration endpoint: provided-method variance/quantification validation, receiverless methods, advanced frames and SDK binders remain pending. The BEP and website remain unchanged.


F56 verification:

| Check | Result / evidence |
|---|---|
| `cargo nextest run -p bex_vm -p baml_compiler2_emit --lib -E 'package(bex_vm) \| test(interface_def_keeps_)'` | **53/53**, 9.202s after 2m33s build/lock time; `/tmp/baml-f56-final-runtime.log`. Includes all VM library tests because resolver substitution context changed, plus five interface-emission cases. |
| `cargo nextest run -p bex_heap --test generational -E 'test(interface_registration_predicate_heads_are_traced_and_forwarded) \| test(interface_owner_and_default_bodies_are_traced_and_forwarded)'` | **2/2**, 0.008s after 59.89s build/lock time; `/tmp/baml-f56-heap.log`. The new test roots only the containing interface and checks subject/constraint/nested-pin heads after minor and major GC. |
| `cargo nextest run -p baml_artifact --lib` | **5/5**; `/tmp/baml-f56-artifact.log`, including version mismatch rejection and payload roundtrip. |
| `cargo build -p baml_cli` | Pass, 3m22s including build-lock waiting; `/tmp/baml-f56-cli.log`. |
| `target/debug/baml-cli test --from crates/baml_tests/baml_src -i dynamic_registration` | **7/7**, 68s including discovery; `/tmp/baml-f56-public.log`. Includes three new public cases for exact requires/projections, blanket-satisfied requirements and builder retry after constraint rejection. |
| `cargo insta test --test-runner nextest --dnd --accept -p baml_tests --lib -- corpus_snapshots` | **1/1**, 17.175s after 3m57s build/lock time; `/tmp/baml-f56-snapshots.log`. Before/after hashes show only `ns_dynamic_impl_registration/bytecode.snap` changed. No snapshot was manually edited. |

The first 19-test selection passed assertions but nextest marked the new emitter test as leaky. Its focused rerun passed cleanly (1/1; `/tmp/baml-f56-emitter-recheck.log`), as did the final 53-test run. No timeout/skip/leak setting was relaxed; the cause of that first process-output leak was not established, and it is not evidence of a VM heap leak. Initial build iterations also caught a usize/u32 slot conversion and a moved qualified-name value; both are fixed.

No SDK IDs or parity requirements changed. These shared compiler/runtime/public BAML checks do not prove native SDK host binding or all-language execution. Artifact format 7 requires regeneration/rebuild before native SDK runs; no native SDK addon was rebuilt in F56. Long local time was spent rebuilding/waiting for the shared artifact lock; the bounded checks completed locally, so no PR/push/CI fallback was initiated. The BEP, website and interactive artifact remain unchanged.


F56 final snapshot check: `cargo insta test --test-runner nextest --dnd --check -p baml_tests --lib -- corpus_snapshots` passes **1/1**, 12.362s after 0.28s build time; `/tmp/baml-f56-snapshots-check.log`. No snapshots remain to review, and no `.snap.new` files remain. Scoped Rustfmt and `git diff --check` pass. The artifact tests took 0.007s after 8.87s build time.

All started commands are terminal. No implementation checkpoint or overall goal is marked complete. Continue with immutable adapter descriptors and supplied-method conformance, then a real registration path through the engine and SDKs; keep registration predicates, ownership and proof checks shared.


### F57 — Checked host adapter registration (in progress)

- [x] Use the retained BAML declaration as the complete method contract, matching host-callable ascription. Record the native-checker/runtime responsibility split in design §7.3 and adjustment F57. Native checker/binder tests remain gates.
- [x] Add VM-local adapter descriptors over `[Self]`; validate explicit methods, defaults, fields, declaration constraints and coherence before publishing.
- [x] Add instance creation with authenticated class layout and all-at-once receiver/callback validation. Keep an opaque receiver even for markers/all-default interfaces; type metadata does not retain instances.
- [x] Replace hand-assembled VM/engine adapters with the checked registration path.
- [x] Finish registration/default/generic/GC/engine tests and final checks (final F57 log, then F58 tests/formatting).

The first four migrated VM method tests pass. Receiver-slot and marker/default-only coverage was added afterward, so that early result does not verify the final implementation. The completed final registration/engine log now passes 12/12; see the evidence below. No C ABI entry point or SDK binder is exposed yet. The VM result needs an owning type handle before crossing an await/permit boundary. Receiverless methods and advanced generic frames remain pending. No native signature-algebra fallback or weakened BAML contract is introduced.


F57 evidence recovered during the documentation review: `/tmp/baml-f57-registration2.log` records `cargo nextest run -p bex_vm -p bex_engine --lib -E 'test(host_registration::tests::) | test(host_interface::tests::) | test(host_interface_tests::)'`, **12/12 passed**, 11.803s after 1m45s build time. This includes the later receiver-slot, marker/all-default and default-override tests. The earlier `/tmp/baml-f57-registration.log` failed compilation while the instance API was changing; the final run supersedes it. No new tests were run by this documentation pass. Final code review/formatting and native binder verification remain open, so F57's final-check box stays unchecked.

### A13 — Internal review: distinguish declarations, native bodies and checked calls

- [x] Reconcile the plan with F57: registration uses the BAML method contract; native compatibility and actual-value checks are separate responsibilities.
- [x] Add a four-stage Source walkthrough to the codegen review, including what binding cannot prove and when a bad completion fails.
- [x] Clarify that non-class BAML receivers do not require host registration or generated class facades, while an ordinary native collection still needs a defined conversion.
- [x] Record the existing final F57 log without claiming a new test run or SDK execution.
- [ ] Complete the existing generated/type-checked/executed review packets before revising the BEP.

Only internal Markdown changed in this pass. No BEP, website, artifact or implementation edits.


### F58 — Owning engine registration boundary

Previous continuation: progress through internal-document reconciliation and recovered final F57 evidence. The active goal now resumes implementation.

- [x] Add engine descriptors with named or retained declaration heads and a `[Self]` template frame, preserving nested dynamic associated identities.
- [x] Root the accepted adapter class and callback interface contracts before leaving the heap permit. Registration owns types only; instances own receivers/callbacks.
- [x] Add engine instance creation with issuing-runtime checks and reuse the VM admission validator.
- [x] Migrate the real dispatcher test through the owning engine operations, including GC between type registration and instance creation.
- [x] Verify type/instance lifetimes, dynamic associated identities, foreign-root rejection and callback dispatch through the engine API.
- [x] Finish scoped checks and document final evidence.

No new bridge ABI or SDK binder exists yet. The descriptor's retained head is a declaration handle, not an Object::Type wrapper. This internal engine capability validates provenance before reading it; a later bridge endpoint must translate its type evidence under the same checks. Receiverless providers and advanced call frames remain pending. No BEP/site edits.


F58 final evidence:

| Check | Result |
|---|---|
| `cargo nextest run -p bex_engine -p bex_vm --lib -E 'test(host_registration::tests::) \| test(host_interface::tests::) \| test(host_interface_tests::)'` | **16/16**, 10.555s after 38.54s build; `/tmp/baml-f58-final.log`. Four new engine registration cases plus the migrated dispatcher and existing VM cases. |
| `cargo nextest run -p bex_engine --test interface_views -E 'test(interface_rejects_incomplete_pins_and_concrete_self_operations) \| test(runtime_created_interface_return_preserves_exact_declaration)'` | **2/2**, 3.407s after 9.73s build; `/tmp/baml-f58-views.log`. Covers the existing named projection wrapper and runtime-created interface transport. |
| Formatting/whitespace | Scoped Rustfmt checks and `git diff --check` pass. Removed the unused F57 Instance test import. |

The new engine template preflight checks nested declaration kind and nominal arity, including projection constraints, before VM substitution. The existing VM checks still own declaration predicates/coherence and unsupported fields. `project_interface_with_type_argument` accepts the existing Named/Definition/Reference evidence choices; live references preserve exact identity and reject a foreign engine. The real dispatcher test consumes the registered callback's retained interface type instead of rebuilding its view by name. A separate test projects a receiver whose associated type is another dynamic adapter class after GC.

Initial `cargo check --tests` caught a missing allocation trait import and a wrong Value accessor; the first nextest build caught unavailable TyTemplate primitive constructors. All were fixed before execution. The intermediate 16-test run passed assertions but nextest marked two VM subprocesses leaky; the final run is clean with unchanged leak settings. This is process-output behavior, not evidence of a retained BAML receiver. The initial broad check was not rerun; final evidence comes from the targeted builds and executed tests above.

All started processes are terminal. No native SDK addon, shared SDK case, parity requirement, artifact schema or snapshot changed in F58. There was no PR, push or CI action. No overall checkpoint or goal is complete. Continue with shared bridge registration and its native implementation source/binder, while retaining the separate advanced-frame and receiverless-method requirements. The BEP, website and interactive artifact remain unchanged. F58's final tests and formatting also close F57's pending final-check work; they do not close the native binder gates.


### F59 — Shared encoded registration operations (in progress)

Previous continuation was verified implementation progress (F58). Continue with the same full goal and BEP/site hold.

- [x] Add explicit template-frame conversion/composition: slot zero is Self, later slots select shared positional type evidence; missing slots reject instead of becoming unknown.
- [x] Expose checked registration, instance creation and retained-type projection through the Bex runtime trait.
- [x] Add shared protobuf messages and target-neutral preparation/execution helpers with output receipts.
- [x] Give adapter type capabilities a dedicated table tag; they are not convertible to ordinary BAML values.
- [x] Capture a whole parsed receiver/callback input batch before target validation; prepared input ownership drains releases when abandoned.
- [x] Verify encoded registration, preparation ownership, foreign-type rejection, instance/projection round trips and real callback dispatch (4/4 shared bridge tests).
- [x] Regenerate affected checked-in protocol clients, run scoped checks, and record results (F59 final results below; F60 extends the protocol separately).

These are shared Rust bridge helpers and wire messages, not yet exported native C/Wasm operations or SDK binders. Callback completion in the integration probe remains in-process external-value completion. Keep explicit native callback-handler frames and receiverless providers as separate remaining requirements. No public SDK IDs are added before the API is exposed in those SDKs.


F59 checkpoint evidence (shared final checks still running):

| Check | Current result |
|---|---|
| `cargo nextest run -p bridge_cffi --test host_registration` | **4/4**, 7.659s; build reported 12m18s including the queued build path. `/tmp/baml-f59-bridge.log`; session 28674 is terminal and reaped. |
| `cargo nextest run -p sdkgen_cpp --test pb_generation --run-ignored only -E 'test(=regenerate)'` | **1/1**, 0.121s; build/lock reported 11m27s. `/tmp/baml-f59-cpp-proto.log`; session 44841 is terminal and reaped. |
| Protocol clients | Rust/Python regenerated through bridge_ctypes' build script. Node `pnpm build:proto` and `pnpm build:copy-proto` pass. Go generated with protoc 27.1 and the repo's pinned protoc-gen-go 1.34.1, then existing internal copies synchronized. Swift generation/manifest update passes. C++ uses its pinned vendored protoc. These are generation results, not native SDK execution. |
| Formatting | Scoped Rustfmt checks and `git diff --check` pass so far. |

The first `cargo check -p bridge_cffi --lib` began before the bridge's new baml_type dependency was added. It finished with that unresolved import under the old manifest graph; the successful subsequent nextest build used the updated Cargo.lock/manifest. A malformed test initializer was corrected before the test binary compiled. No runtime failure was skipped or relaxed. Go's initially installed generator was 1.36.6; rerunning with the pinned 1.34.1 removed unrelated generated-file churn. No PR, push or CI action occurred. Time was spent on dependency rebuilds and the shared artifact lock; the actual executed tests took seconds.

The engine descriptor now resolves explicit type evidence once, composes it into the `[Self, ...]` template, and uses the same F58 validator. An entire retained interface can occupy an explicit slot. The instance input decoder now has an explicit batch API, rather than pretending the receiver/callback list is a typed BAML array. Adapter-type capabilities fail ordinary BAML value conversion.

**Resume this live command:** session `13583`, `/tmp/baml-f59-shared.log`:

```sh
cargo nextest run -p baml_type -p bex_engine -p bridge_ctypes --lib -E 'test(frame_conversion_tests::) | test(host_registration::tests::) | test(host_interface_tests::) | test(handle_table::tests::) | test(value_decode::tests::) | test(transfer::tests::) | test(ty_decode::tests::)'
```

Last observation: the session is live and compiling shared dependencies. It has not reported a test result. All other processes started in F59 are terminal/reaped. After this command completes, fix any failures, verify C++ generated-source drift with its normal pb_generation checks, and finish the scoped formatting/evidence pass. Do not claim F59 fully verified yet.

Next implementation: export these operations through native C/Wasm admission, cancellation and receipt-delivery paths, then implement SDK registration sources/binders and canonical SDK cases. Shared Rust helpers are not those exported endpoints. Preserve the full receiverless/advanced-handler, all-language, client/Agent, media and alias scope. No SDK IDs, parity requirements or bytecode snapshots changed in F59. BEP, website and interactive artifact remain unchanged.


F59 final evidence: session 13583 completed successfully, **66/66** in 9.268s (`/tmp/baml-f59-shared.log`). C++ normal `pb_generation` checks completed **2/2** in 0.102s after 26.96s build (`/tmp/baml-f59-cpp-check.log`, session 44150). Both processes are terminal/reaped. These close the pending F59 checks; F60 changes require separate verification.

### F60 — Native administrative delivery

- [x] Give register/create/project one private request discriminant and an owned response envelope. Keep registration metadata separate from ordinary BAML values; reuse the existing structured error/panic classifier.
- [x] Add Python and Node native sync/async entry points using captured runtime authority, synchronous preparation, and existing aggregate result receipts. These are private bridge methods, not generated public binders.
- [x] Add native tests for registration, associated-type evidence, callback/default dispatch, discarded result ownership, rejection and replaced runtimes. Python also probes failed async scheduling.
- [x] Build and execute the new native tests; regenerate native stubs/protocol consumers and check them.
- [x] Verify the final shared administrative envelope/error tests and record evidence.

The C receipt ABI and Wasm/browser/native ports remain pending. Registration executes no user methods and has no application call ID; its future owns input/output cleanup. Method invocation retains the existing call-ID/cancellation path. Cancellation before/after scheduling and SDK shutdown still require each native bridge's evidence; do not infer all-language support from these two entry points. No generated host implementation/binder or canonical SDK ID is added by this slice.


F60 final evidence:

| Check | Result |
|---|---|
| Shared bridge administrative operations | **5/5**, 7.220s after 7m36s build/lock; `/tmp/baml-f60-shared-final.log`. Includes structured failure and receipt cleanup, retained type evidence and real dispatcher completion. |
| Python private native operations | **6/6**, 22.32s; `/tmp/baml-f60-python-contract.log`. Both synchronous/asynchronous administrative entry points, real callback/default invocation, retained receiver state, off-contract completion, rejected inputs, discarded output and failed async scheduling. |
| Python runtime replacement | Existing two cases plus the initial six new cases **8/8**, 33.84s; `/tmp/baml-f60-python-verified.log`. The six new cases were subsequently rerun after adding wrong-return assertions (row above). |
| Node private native operations | **5/5**, 22.21s; `/tmp/baml-f60-node-tests.log`. Both administrative entry points, real callback/default invocation, receiver state, off-contract completion, discarded result and rejected/replaced-runtime input release. |
| Node native type declarations | New test file passes `tsc --noEmit --target ES2022 --module NodeNext --moduleResolution NodeNext --skipLibCheck`; `/tmp/baml-f60-node-types.log`. This checks native protocol usage, not generated Host contracts. |
| C++ protocol generation/check | Regeneration **1/1**, 0.088s (`/tmp/baml-f60-cpp-proto.log`); final drift/lite checks **2/2**, 0.086s (`/tmp/baml-f60-cpp-check.log`). |
| Build/generation/formatting | Combined Python/Node native build and Python stub generator pass (7m26s); NAPI packaging/type generation passes (9m13s including queue/rebuild). Rust/Python, TypeScript, pinned Go and Swift protocol clients regenerated. Scoped Rustfmt, Python Ruff and `git diff --check` pass. |

The initial Python native run passed four cases and failed two because the **test request** used `type_ref` instead of the protocol's `type_reference`. Correcting the request made all six pass; no engine validation was relaxed. The final native probes now check that closing type/instance SDK handles leaves a retained view callable, the original host object's methods update its owner-language state, and changing that object's body behavior to return the wrong type produces `HostContractViolation`. These are ordinary host methods; advanced GenericCall frames remain open.

Native entry points are `_host_operation`/`_host_operation_sync` in Python and `_hostOperation`/`_hostOperationSync` in Node. They use `BamlEncodedResult` for administrative outcomes. They do not provide a generated public `bind` API. Synchronous administration can register JS callbacks because it runs no user bodies; subsequent JS method invocation uses the asynchronous call path. Tests use private wire helpers; they are not examples of the intended end-user API.

All processes started in F60 are terminal/reaped. Builds waited on the shared Cargo artifact lock; no command was restarted simply because a poll timed out. There was no PR/push/CI action. No canonical SDK test ID, parity baseline, artifact version or bytecode snapshot changed. No BEP, website or interactive artifact was edited.

Next: build generated implementation sources/binders and canonical shared SDK cases on these operations; carry the complete advanced generic frame and receiverless providers; finish C/Wasm/native ownership delivery and all-language execution. Keep compiler aliases, media/PIL, clients/Agent calls, lifecycle/scopes/cycles and the other full-plan gates in scope. F60 does not complete any broader acceptance checkpoint or the overall implementation goal.


### F61 — Checked obligation identities and Python explicit binders (in progress)

Previous goal turn was verified implementation progress (F60). The full scope remains active; BEP/site hold unchanged.

- [x] Replace per-callback type duplication with a descriptor-ordered list of exact retained interface types. Every implementation is represented, including markers/all-default interfaces. Callback slots carry an implementation index and authored member name.
- [x] Migrate existing VM/engine/shared bridge/native tests to that response shape. The first shared bridge run passes 5/5; final expanded checks remain below.
- [x] Add a private Python binder using checked obligations, exact retained projection evidence, full input/output ownership, codec capture, native calling-convention inspection and optional default overrides. Properties are rejected without evaluation; method bodies are not trial-run.
- [x] Add real binder probes for overlapping member names, all-default interfaces with freshly imported associated types, and early native convention/property rejection. Native Python suite passes 9/9 in 30.40s (`/tmp/baml-f61-python-final.log`).
- [ ] Finish generated ordinary Python Host contracts and explicit Ref.bind, with native positive/negative type-checking and real generated execution.
- [ ] Add canonical shared SDK cases and verify parity requirements without relaxing other bridges.
- [ ] Finish protocol/native regeneration and all relevant existing checks; update evidence and limitations.

Discovered prerequisite: callback member names alone cannot identify operations from separate implemented interfaces. Also, reconstructing an associated portable definition for projection would import a fresh type; a marker/all-default interface has no callback from which to borrow the original type. F61 returns every realized obligation once, in submitted implementation order, and indexes callback slots into that list. The engine and bridge preserve one identity source instead of duplicating it in each callback.

The private binder already accepts registration type evidence and multiple obligations. Public typed emission is staged: ordinary interfaces and fully ordinary required-interface graphs are the first generated surface. Interfaces needing generic/associated choices, concrete Self, generic methods, conflicting member contracts or fields do not gain an Any-shaped binder. Typed associated descriptors, advanced handlers, receiverless providers and auto-binding remain required follow-up work. The generated implementation base/factory, host context injection, scopes and automatic identity reuse are not supplied by this initial explicit binder. C/Wasm/other bridge ports remain open. No broad checkpoint is complete.
