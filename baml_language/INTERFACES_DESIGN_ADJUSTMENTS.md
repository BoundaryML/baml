# Interface design adjustments and findings

This log records changes needed after the BEP discussion and findings that affect [the implementation plan](INTERFACES_IMPLEMENTATION_PLAN.md). The updated contract is [the design](MEDIA_INTERFACES_AND_BRIDGES_DESIGN.md). This log distinguishes proposed API changes from implemented and verified behavior.

## Changes agreed in the BEP discussion

### A1. Ordinary host methods can remain local

The original host object may be called directly by its host language. BAML invokes that same receiver through a checked bridge call. Both paths observe one owner-held state, but only the bridge path receives bridge validation, lifetime admission and cancellation semantics. Calls on generated BAML-owned proxies still cross the bridge. A BAML default invoked from a host object also enters BAML.

Updated design §1.1.1 and the guide. We do not need an interception layer that turns every Python override into a proxy. Local and incoming calls may overlap; no implicit lock or transaction is added.

### A2. Generic methods do not automatically require public frame handlers

The earlier design required `GenericCall` for all host method-generic members in several languages. That was too strong. An internal realized call frame is still necessary, but a generated adapter can hide it where the entire contract is representable through native methods and checked values.

Updated design §11.3, the shared helper contract, alternatives, and guide. A native adapter must handle all allowed specializations and bounds; a finite specialization table does not establish universal conformance. Python/TS erasure or a missing generated native model is not, on its own, evidence that the user must handle a frame.

### A3. Separate advanced handler registration from ordinary implementations

The old `EchoHost.echo(GenericCall)` looked like a normal `echo(value)` method but accepted different arguments. The BEP now uses an explicit `Echo.bind_handlers({ echo: handler })` path returning an ordinary callable interface ref. The original warning telling users not to call an apparent implementation method is removed from the guide.

`GenericCall` describes one invocation. The bridge constructs it; `argument(name)` returns checked values and `return_value(...)` produces the checked `HostOutcome`. Neither changes object ownership or the caller's method signature. Generate separate handler contracts/builders rather than silently changing ordinary host method arguments.

### A4. Field interop remains outside this PR

A remaining §11.1 table still promised live get/set operations, contradicting the agreed scope. Corrected it: retain metadata, keep methods on BAML-owned field-bearing receivers usable, and reject native field-requiring bindings until that protocol exists. Stateful authored methods and ordinary copied record fields remain supported.

### A5. Remove the unexplained mandatory Rust caller context

The BEP required `&ctx` on Rust methods/factories while omitting it from free functions. Current Rust generated calls require neither `CallContext` nor `HostCallContext`; `sdks/rust/bridge_rust/src/error.rs` also already uses `Error<E = Infallible>`. The required caller context was a proposal inconsistency, not an existing SDK restriction.

Local draft revision: ordinary Rust calls use the receiver's runtime or selected SDK runtime without `ctx`; opt-in `_with_options` entrypoints provide per-call controls consistently. The bridge supplies the separate incoming `HostCallContext` to implementations for cancellation and tracked work. It is not a lifetime scope. Generated option-name collisions and host injection still require checkpoint-0 validation.

The local BEP now shows Python awaitable/exception, TypeScript Promise, and Rust Future/Result shapes immediately after the consuming examples, including live client references, copied extraction results, and copied `RunResult` with a live journal. Rust `throws never` uses the existing `Infallible` spelling. This revision is **not published**, per the user's request to address more feedback before updating the website.

## Planning choices to settle before broad codegen

Current feedback follow-up: [the codegen review note](INTERFACES_CODEGEN_REVIEW.md) collects proposed signatures and remaining validation. No BEP or website changes are authorized by this follow-up.

### P1. Context injection must allow ordinary local Python calls

Proposed adjustment: context-aware Python host methods accept an optional keyword-only `ctx`; the bridge supplies it and local callers may omit it. The main Python guide example now shows this shape. Final naming when a BAML method already has a `ctx` argument, and precise generated typing/default behavior, remain checkpoint-0 probes. This is a proposed API decision, not an already tested adapter.

### P2. Record native generic adapter eligibility per member and backend

Test primitives, generated/runtime-created records, live references, generic bounds, exact error types, and a runner's spec/result relationship. Decide between an ordinary native adapter, an ordinary method over checked dynamic arguments, and an explicit advanced handler. Host language labels alone are not a support matrix. No such eligibility result is claimed yet.

## Unexpected repository findings

### F1. The case parity lint does not run tests

[`tools/sdk-parity-lint/README.md`](../tools/sdk-parity-lint/README.md) describes a deterministic AST inventory across ten environments. It compares exact `<category>/<name>` declarations with a baseline, tolerates existing gaps, and does not compare assertions or execution results. Its `SDK_PARITY_LINT(skip)` annotation waives absent environments for the canonical case, so it is broader than a local skip.

Plan adjustment: require both canonical declaration parity and native execution, inspect all new rows rather than just aggregate ratchet success, and keep portable TypeScript cases ungated across Node/Chromium/workerd. CI's separate `sdk-test-coverage` job checks SDK directory matrix entries, not case parity.

### F2. Declared Rust tests can be disabled; new Java fixtures can be ignored

[`sdk_tests/harness_setup/src/rust.rs`](sdk_tests/harness_setup/src/rust.rs) has `TEST_MODS` and `Gate::Later`; disabled source files still exist for parity discovery. Java uses explicit green fixture lists for actual compilation/JUnit execution. Thus a new test declaration can count without executing.

Plan adjustment: enable each new Rust module and add new Java fixtures to the real compile/runtime gates; check generated discovery and skips during acceptance. Inspect other backends' equivalent gates too. No placeholder or source-only test can satisfy a portable runtime case.

### F3. C# does not currently consume the common fixture layout

[`sdk_tests/harness_setup/src/csharp.rs`](sdk_tests/harness_setup/src/csharp.rs) generates an explicit list of C#-specific fixture projects with their own BAML inputs. Native consumers run through Rust tests in `sdk_tests/crates/csharp/src/lib.rs`. The parity tool's C# rule scans Rust and assigns every existing wrapper to `integration`, so it cannot match a new `interfaces/<case>` merely by naming the function similarly.

Plan adjustment: add shared BAML fixture support for the new interface/client cases and per-case native execution wrappers. Extend discovery to the new per-fixture wrappers while preserving old `integration` IDs. Test that change in the parity tool; do not waive C# or duplicate the BAML test sources. This is bounded harness work, not a rewrite of all existing C# tests.

### F4. The Rust SDK harness still launches `cargo test` internally

The Rust setup scripts prewarm generated projects using `cargo test --no-run`, and the harness runner emits `cargo test --manifest-path Cargo.toml`. This conflicts with the supplied nextest-only testing rule even when the outer invocation uses nextest. The fixture commands also change into generated project directories.

Plan adjustment: replace that nested execution/prewarming path with nextest, pin manifests, run Cargo from `baml_language/`, preserve native library/target settings, and check that nested test selection does not recurse into SDK setup. Update Unix/Windows scripts and labels together. No such harness commands were run during planning.

### F5. Ruby has loader tests, but no full interface codegen target

[`sdk_tests/crates/ruby_sorbet/src/lib.rs`](sdk_tests/crates/ruby_sorbet/src/lib.rs) runs loader/lifecycle tests. Ruby is represented in the SDK CI matrix, but not in the ten-environment declaration parity tool. The earlier phrase “Ruby is a scaffold” must not be read as “no code exists.”

Plan adjustment: preserve the existing loader against the ABI cutover. Full Ruby generated values/calls/interfaces remain outside the agreed scope until its base SDK exists; do not count its loader test as interface conformance.

### F6. Markdown validation does not inspect these untracked documents yet

`scripts/validate_markdown.py` permits README files and explicit `.markdown-whitelist` entries, and only inspects Git-tracked paths. The current design/guide/plan/log are untracked. Add exact entries for documents intended for the implementation PR, rather than claiming a passing validator already covers them. This pass validates their structure and new local links directly.

## Execution policy added by the user

Local runs taking roughly four minutes or more may move to the single draft PR's CI when that provides faster or otherwise useful feedback. Record commit and job provenance. Snapshot candidates may be retrieved through a helper using GitHub artifacts/logs, reviewed and applied, then verified on the resulting head. The plan describes handling stale/truncated results and avoids automatic acceptance of unrelated snapshot changes.

## Still unresolved

- The exact native generic adapter eligibility table and final host context/handler signatures.
- The minimal C# shared-fixture runner/discovery implementation and generated Rust nested-nextest wiring, to be validated before relying on their test results.
- Runtime implementation details such as receiver/view table layout and test-only collection/drain observability, while preserving the already agreed lifetime semantics.

The initial planning pass created no production source changes. Implementation has since begun with the compiler alias registry foundation; see the plan for its narrow test results. Interface runtime behavior, new SDK tests, CI jobs, and the PR remain outstanding.

### Rust never-type follow-up

The user correctly noted the native `!` type. Upstream stabilization merged August 24, 2026 (https://github.com/rust-lang/rust/pull/155499), including making `Infallible` an alias to `!`. The repository currently pins Rust 1.93.0. A direct rustc probe of `Result<String, Error<!>>` on that toolchain fails with E0658. Prefer `Error<!>` once the generated SDK's minimum supported compiler includes stabilization; use `Error<Infallible>` while targeting the current compiler. Do not confuse stable diverging return syntax `fn exit() -> !` with support for `!` as a generic argument. No toolchain or published BEP change made.

### A6. Review follow-up: generated types and non-class receivers

Pending BEP revision, kept local: show paired BAML declarations, generated caller/host signatures, complete invocation examples, and native results. Cover associated-type projections in Python, TypeScript, Go and Rust, including exact runtime pins and associated errors. Do not imply native generic annotations establish runtime membership. Introduce `GenericCall` by name on first appearance and link forward to its advanced-handler explanation. Show `ResponsesClient` as a generated facade over the existing BAML implementation, separately from host implementation contracts.

The repository implements `Iterable` for `T[]` and `string` in `baml/ns_iter/iter.baml`; arrays produce `ArrayIterator<T>`, which implements `Iterator`. This is a useful acceptance case for builtin receivers and out-of-body implementations, rather than assuming every implementation is a class. A BAML array returned as `Iterable<Item=string, Error=never>` needs a checked interface ref retaining that BAML array; an ordinary `string[]` return remains copied native data. This does not add shared native list mutation.

A native list supplied where an interface is expected is a separate conversion question. Proposed conservative boundary: require an explicitly typed value-to-BAML conversion or a declared host adapter until a backend can generate a sound convenience conversion. Do not infer BAML conformance from Python/JS iteration behavior or inspect the first element to guess an empty/heterogeneous collection's type. A value conversion creates BAML-owned data; a host adapter retains the host implementation. Final helper names and acceptance types remain to be probed.

Failures should be detected at the earliest boundary with sufficient information: invalid BAML implementation at compilation; unsupported static SDK projection at generation with a declaration/member diagnostic; missing or wrong runtime pins/implementation at binding or argument encoding before dispatch; actual host return contract violation at completion before returning it to BAML. Existing-reference validity is rechecked at invocation. Never silently omit a builtin implementation or method. These failure-stage and native-list policy examples belong in the BEP; full codec details remain in the design.

### A7. Internal feedback revision applied

Expanded the authoritative design with non-class receiver rules (§6.1.1), associated-type and associated-error projections across Python/TS/Go/Rust (§9.2.1), and an earliest-failure table (§9.14). Fixed the remaining helper-contract sentence that required Rust caller context. The guide now distinguishes a generated ResponsesClient facade from host implementation contracts, shows generated signatures and native results, annotates GenericCall/HostOutcome, and uses snake_case for authored functions.

Concrete builtin arrays retain their copied-data codec despite implementing Iterable. Interface positions retain the BAML array through an ordinary checked view. For native collections, the initial supported conversion route is a BAML factory with a concrete array input and interface output; this makes ownership and element type explicit without freezing an unprobed SDK helper. Added canonical cases and compiler/type-check gates to the plan.

Associated Error parameters remain in interface ref/input types even where exceptions are untyped. `IteratorNext<T>` is the proposed generated anonymous union name in Go/Rust; its variants/accessors and the host-context injection details still need codegen probes. This pass establishes documentation requirements, not completed native bridge support. The local and published BEP were left unchanged.

### F7. Builtin AST lowering discarded invalid type arguments

`baml_compiler2_ast::lower_type_expr` had its own string mapping and dropped primitive/json type arguments and associated bindings. The implementation now consumes the pure compiler alias registry through a dependency on `baml_type` (which has no AST/HIR query dependency) and reports invalid arguments before the AST loses them. Map lowering's two CST paths converge there as well. `infer::static_qualifier_ty` no longer maintains a separate primitive/media name table. This implements the existing canonical-alias design; it does not add a second accepted source spelling or change the planned source-name ban.

The first native Rust interface fixture compiles its test module and fails on the missing interface APIs, while the BAML fixture itself compiles. This confirms the generator/export gap remains observable; successful fixture discovery is not native conformance. The plan records the separate evidence.

### F8. Associated projections must survive SDK export

Follow-up source inspection confirms that `crates/baml_ide/src/symbol_pool.rs::convert_tir_leaf` maps `AssociatedTypeProjection` to `Unknown`, while `baml_codegen_types::Symbol` has no interface declaration variant. The compiler's `PackageInterface` already distinguishes interface declarations, associated bounds/defaults, required/default methods and out-of-body implementation rules. A method such as `decode() -> Self.Output` therefore needs a richer shared export path before the language generators can produce the promised types.

Updated design §12.C and checkpoint 2: preserve symbolic projections, scoped parameters, bounds, effects and implementation targets until they can be resolved for the receiver/call. Do not erase valid generic types or independently rediscover implementation rules in each generator. Added a review checklist linking exact generated examples, context/handler distinctions, provider clients and non-class receivers to their required proof. This follow-up changes internal documentation only; the BEP remains on hold and no new implementation or native-test result is claimed.

### F9. Shared export exposed incorrect callable error metadata

The new lossless export initially rejected `baml.iter.Iterator.collect throws`: its declared `Self.Error` survived signature lowering, but `callable_throws` lowered the same clause with only a generic frame, losing the owner/bound scope and producing a compiler error type. Fixed that query to include bounds, concrete Self and the implementation target. The export test now checks both the declared and effective throwing effects. No `Unknown` fallback was added.

Implementation choice: `SymbolPool.interfaces` stores the semantic graph separately from projected native symbols, using the existing `RuntimeTy` family. This avoids forcing symbolic projections through `CodegenTy`, whose current contract excludes them, and lets non-class implementation rules retain their receiver patterns. Rust's native-only signature rewrite passes preserve the graph. Native generators must still consume it; declaration emission, implementation policy, source presentation, reachability and runtime invocation are not completed by this change.

Source-less compiler-built packages intentionally upgrade VM/IO callable linkage from reserved to linkable. The source/source-less comparison checks this specific compiler policy while requiring the declaration types and implementation rules to survive. Arbitrary mounted packages do not acquire that trust merely by containing builtin-looking metadata. The BEP is unchanged.

### F10. Bound-method frames and argument checks are separate requirements

The existing implementation work found three assumptions that fail for interface methods: lowering a generic method reference as an ordinary function name, discarding bound-method arguments during specialization, and reconstructing invocation arguments from the receiver's class. A primitive receiver has no class frame; a BAML default also needs its resolved Self/interface arguments. The worktree now preserves that complete frame, but this does not finish checked invocation.

At the documentation review checkpoint, the strengthened engine test still accepted `Int(9)` through a captured `echo<string>` handle. Contextual argument conversion alone does not establish that a value satisfies the retained signature. Validate against authoritative realized runtime types before entering the body; do not re-infer a fixed specialization, trust a display-name annotation, or exempt live handles from validation. Apply the same contract to interface methods and callable handles, with matching result/error checks and transfer rollback.

The stateful default-method test passes across major GC and pass-back. The generic test fails before reaching its later output-only `empty<string>()` assertion, so that behavior remains unverified. The plan records the failing check and adds portable case names for specialization retention and pre-execution argument rejection. This review changes internal documentation only and leaves existing source edits in place.

For the eventual BEP revision, design §9.2.1 now requires complete generated examples with caller/input/implementation roles, associated substitutions, construction, return shape and error channel. Proposed signatures remain labeled as proposals until verified against generated output. The local BEP, published BEP and website remain unchanged.


### F10 implementation follow-up: resolved static bound-method checks

The next implementation pass exposed a fourth issue: interface-default signature metadata omitted the implicit Self parameter from its generic list. The body and bound method held `[Self, T]`, but metadata reconstructed only one slot and widened the remaining T to `unknown`. The emitter now preserves Self in that order. The engine validates actual arguments against the anchored signature before execution; a separate class-only return-type substitution is no longer needed.

The strengthened cases now pass, including output-only specialization, exact associated pins on live inputs, rejection of forged annotations, literal membership, unchanged state after rejection, and release of external roots with the runtime still open. A further test caught over-rejection of explicit opaque host payloads: these must be checked as the internal `$rust_type` represented by RustData, not mistaken for BAML classes or accepted under arbitrary interface contracts. That case now passes too. Exact commands and counts are in the plan.

This establishes static compiled receiver/callable behavior, not the complete interface ABI. Runtime-created implementations still need receiver-world-aware resolution; declaration roots, optional scope authority, first-class views and generated native APIs remain outstanding. Keep these gates separate from the passing bound-method tests.


### F11. A retained interface method needs its own caller contract

The first retained-view slice now carries a receiver, exact interface type, rooted declarations and the selected implementation world through `BexExternalAdt::Interface`. Primitive and array receivers use the same representation as classes. Static-source engine tests pass for method/default dispatch, owner-side state, copied records with live children, pass-back, method capture and collection. This is engine groundwork; it does not establish generated bridge support.

A follow-up found that binding an ordinary method and invoking it through the callable entry loses the interface's declared signature. A concrete method may accept `unknown` where the interface accepts `string`, or return a concrete class where the interface promises another interface. Using that concrete signature would accept invalid checked calls and copy objects that should remain live. Bound methods now optionally retain a separate realized interface function contract. The emitter uses the compiler's canonical method signature/frame and effect query, rather than independently lowering signature fragments. Callable entry uses the retained caller contract for arguments and outbound results/errors while preserving the implementation frame for execution.

Bare covariant result `Self` becomes the current checked interface view; associated projections keep concrete Self for exact reduction. The GC head walker traces and forwards the retained contract. Interface reflection also needs a complete associated-member list: its existing list of associated *bounds* omits unbounded members, so it cannot validate complete existential pins. Artifact/cache formats are bumped for the stored metadata change.

Still outstanding: receiver-world-aware type checks and associated projection throughout invocation, retaining that world when a method outlives its view, anchored outbound contracts for runtime-created declarations, automatic host registration and scope teardown, direct native method entrypoints, and generation in every bridge. The new common CFFI interface tag is only protocol groundwork until all consumers and wrappers are updated. Do not infer full support from static-source tests or from regeneration of the Rust/Python schema alone. No BEP or website edit.


### F11 follow-up: optional slots, reflection and interface-valued errors

The canonical function-subtyping tests show that required parameter names are insignificant, while optional names identify slots: an implementation may add or reorder optionals. A checked method therefore cannot require identical arity or pass declaration slots straight to the concrete body. The engine and VM now share a checked-method slot mapper, retaining implementation-only defaults. The new engine fixture exercises named invocation and captured-method pass-back. Required bodyless interface declarations currently reject default expressions; the fixture uses a legal default method to declare its optional parameters.

Reflection must report the retained interface signature as well. Otherwise a captured `accept(string)` would reflect the implementation's wider `accept(unknown)` even when direct host invocation checked it correctly. The pass-back test now checks reflection before calling the method.

Existing callback/union tests expected copied concrete classes for interface-typed values and errors. They are being migrated to verify the selected interface arm, rooted handle ownership, callable error methods, and preservation of the original concrete identity on pass-back. Concrete-record expectations remain for concrete types. These are intentional projection changes, not a reason to preserve the old ABI. Broader callable-subtype cases, including native higher-order calls through a narrower optional-parameter function type, remain an explicit invocation test requirement.


### F12. Dispatch needs the caller's argument layout, not the callee's arity

The next probe found a language/runtime mismatch beyond the new host entrypoint. BAML permits an interface method or function subtype to have additional/reordered optional parameters. `VirtualCall` currently assumes the interface fixes the implementation's arity, and `CallIndirect` reads its argument count from the runtime callee instead of the call site. Both assumptions are false under the existing type-system rules.

The enabled engine case `checked_method_narrower_callable_and_native_arguments` compiles a default `Formatter.format(value, prefix=...)` overridden by `Fancy.format(value, extra=..., prefix=...)`. It gathers three outcomes before asserting, so one failure cannot hide the others:

| Path | Observed failure |
|---|---|
| Ordinary BAML `formatter.format("Ada", prefix="<")` | Implementation expects 4 slots including self; virtual call supplies 3. |
| Captured method passed as `(string) -> string` | Indirect call reads too many stack slots and delivers a closure where the implementation expects a string. |
| The same callable passed to `Array.map` | The native callback supplies required arguments only; the checked-method mapper rejects the incomplete full-layout argument list. |

Required correction: represent caller argument layout explicitly through MIR/bytecode dispatch. Required positions and named optional positions are sufficient for slot mapping; type arguments still use their separate realized frame. Use one mapping rule for direct/virtual/indirect calls, checked engine entry and native higher-order invocation. Fill absent implementation optionals with omission sentinels so the selected body evaluates its defaults. Validate unsupported layouts before reading unrelated stack values or entering the receiver. Do not restrict valid function subtyping or add a Python-only adapter to mask the mismatch.

The implementation should inspect constant-call optimization too: resolving a function value to a known body does not prove that the caller and body share an argument layout. Cache a mapping only with the corresponding call-site layout and resolved target. Preserve explicit runtime IDs, generic frames, omitted arguments and cancellation behavior. No new syntax is necessary to express these existing legal calls.

At the initial checkpoint this was the immediate next implementation task, ahead of native generator rollout. The enabled test failed then; its correction is recorded below. The broader protocol checkpoint is not complete. The nine earlier retained-view cases and compiler snapshots pass independently. No BEP or website edit.


### F12 implementation follow-up: call layouts replace optional adapters

The worktree now carries `CallLayout` (required positions and optional names) from checked call plans through MIR to a serialized bytecode table, translated to compact PCs at load time. Direct constant-call optimization keeps the same layout. Virtual calls derive the declared method layout, and native continuations distinguish positional callback arguments from reflection's already checked named slots. All use the same complete slot-map validation and omission sentinel. Type arguments and explicit runtime IDs remain separate.

The original three-path probe now passes. A further ordinary-function test caught a defect in the old source coercion closures: they discarded optional parameter metadata. Those closures are now removed instead of maintained alongside runtime mapping. The checker records the function-shape coercion; the function value itself retains its identity. Source calls, bridge handles, named/reordered options, native map continuations, reflection and specialization pass the first focused rerun (11 tests). The final strengthened engine set passes 51/51; reflection/type checks pass 35/35, and layout serialization/compact-PC checks pass 3/3. Corpus snapshots pass with three reviewed removals of adapter code. The plan records exact commands and the isolated rerun of one nextest output-handle leak report.

Indirect calls require explicit layouts, including synthetic host-entry trampolines; a missing layout is an error rather than an instruction to infer stack consumption from the callee. This fixes argument layout, not the remaining interface registration, dynamic receiver-world or native SDK codegen gates. No BEP or website update.


### F13. Runtime package mounts lose interface methods before bridge dispatch

A new engine test compiles `RuntimeCounter implements app.Counter` at runtime, returns it as the original `Counter` interface, collects garbage, invokes an inherited method and passes it back to static BAML. The first actual compilation attempt fails before dispatch: mount enrichment writes an interface stub without methods and emits default methods as free functions under `ns_Counter`. That namespace shadows `Counter`, producing E0099 and E0115. The serialized `PackageInterface` already contains the missing method signatures; the loss happens when constructing link stubs.

Work in progress: keep interface methods inside their interface declaration, relocate required and default method targets, and preserve method parameter types, optional modes, effects, generic bounds, associated declarations and requirements there. Link stubs never own the executable defaults; the original package's body remains authoritative. The test remains enabled while this prerequisite and subsequent receiver-world checks are completed. No BEP or website update.

### F13 implementation follow-up: declaration ownership through invocation

The runtime-created Counter now compiles, invokes its inherited default, passes back to static BAML and releases its external roots while the engine remains open. Mounted interface stubs preserve their required/default methods, associated declarations, requirements and generic bounds. Implementation lookup follows the packages owning the exact declarations participating in the obligation, as well as the lexical dependency closure; a display name does not select a world.

A second probe found two downstream losses. Callable entry used the static global table for a runtime package's function wrapper; it now uses the VM's package-aware function resolver. Output conversion then reduced the retained return type to a name and attempted to look it up in the static program. Root threads now retain and trace the exact return/error contract across suspension and moving GC. Interface projection and nested array/map/record conversion preserve those heads. Union selection checks exact membership before accepting a name-shaped candidate.

The strengthened engine tests cover fresh same-spelled interface declarations, direct/nested/union results, interface-valued errors, associated/generic mounted methods, GC and release of external roots. The suspended-host-call test exposed the separate effect issue below. Native interface codecs, exact inbound dynamic contracts, transactional adoption and cross-language lifetime gates remain open; these engine tests do not establish SDK support.

### F14. Callback effects must distinguish inference from an explicit `never`

The suspended callback probe initially failed before dispatch: a callback required as `() -> null throws never` was materialized as `throws unknown`, so checked callable entry rejected it. Callback conversion had been widening every bottom effect to unknown to accommodate foreign exceptions. That conflates an unspecified effect with an explicitly empty error set. `TYPE_SYSTEM.md` says `throws never` may panic but cannot throw a declared error.

Removing that widening exposed why the workaround existed: runtime function metadata re-lowered authored type refs rather than the elaborated compiler signature. An omitted callback effect therefore became `never`, and its synthetic generic slot disappeared from the metadata. Function metadata now consumes the canonical elaborated signature, then uses the existing runtime generic layout. That layout deliberately omits compiler-only implicit effect parameters and lowers those effects to unknown. An explicit never is preserved and an unexpected native exception becomes a host contract violation. Do not add phantom runtime slots for erased compiler parameters. Preserving inferred implicit effects exactly at runtime remains a separate limitation; explicit effects and associated error types must retain their existing exact contracts.

Verification is in progress. Keep explicit-never acceptance, explicit-never violation, omitted-effect native errors, reflected signatures and GC during suspension as separate assertions. This changes internal implementation/design only; the BEP and website remain unchanged.

F14 verification follow-up: the explicit-never and omitted-effect cases pass together, including the forced-GC suspension case. Corpus verification caught generated init/test helpers with no authored return annotation; their unit result is now explicit in metadata construction. The final metadata path uses `RuntimeGenericLayout`, so compiler-only effect parameters remain erased according to the current runtime policy and do not shift real generic slots. This limitation is recorded in design §11.8. The 48 accepted bytecode snapshot updates show the corrected callback effects and Self receiver slots; exact commands and remaining gates are in the plan.

### A8. Feedback continuation: distinguish native types from checked bindings

Added `INTERFACES_CODEGEN_REVIEW.md` as the short internal review entrypoint. It pairs Source's Output/Error substitutions with caller and implementation signatures in Python/TS/Go/Rust, explains generated provider facades before host implementations, and introduces GenericCall with concrete argument/completion types before discussing handlers. It also records the failure stages for array/string implementations. These are proposed signatures; native generated-code validation is still open.

Corrected the technical design's Python Host protocols to use the same optional keyword-only context as the guide. The SDK supplies context; ordinary local calls may omit it. The PIL example now separates local execution from SDK-tracked work. Generated context-name collisions still need a concrete naming policy and compile-check fixture.

Clarified a limit of the Rust associated-token convenience: deriving a token from a native type works only when that type's sealed codec identifies one exact BAML type. Many runtime-created BAML types can share one DynamicValue wrapper, so their binding needs a checked per-instance descriptor and codec. Added an explicit planning gate rather than implying that the static factory solves this case.

Associated Error bindings remain exact even in native languages with untyped error channels. Non-class iteration follows the declared `Item | Done` union; no invented wire tag can disambiguate an Item that itself includes Done. These are internal design clarifications, with no change to field/mutation scope, implementation source, BEP, or website in this continuation.

### A9. Review continuation: show construction, invocation and separate error contracts

Expanded the internal codegen review, without editing the BEP or website. A BAML Source implementation and factory now sit beside proposed caller signatures for Python, TS, Go and Rust. The factory's own `throws never` and its returned ref's associated `Error=ReadError` describe different invocations; the generated signatures must preserve both. This is also an example of using a returned BAML implementation without host registration.

Added a complete advanced Echo handler sequence with the BAML declaration, imports, `call: GenericCall`, `DynamicValue`, `HostOutcome`, binding and the ordinary string result. The handler has its own type and registration API. Native generic T is not proof of codec availability; ambiguous or runtime-created type evidence remains a specialization-API gate.

Clarified that invocation context applies to standalone host callables as well as interface methods. Context-free callbacks keep their convenience through an explicit adapter mode. This extends the documented replacement adapter requirements; it is not a claim that today's callback API already supplies context.

Added an empty-list-to-Iterable factory and the concrete-Self/receiverless boundary from TYPE_SYSTEM.md. Non-class receivers use the same checked implementation rules as classes. Two existential refs cannot establish a shared concrete Self merely by implementing the same interface. Added corresponding plan gates. Validation in this pass is document/source consistency and Markdown structure; generated signature compilation and native SDK execution remain open. No production code changed in this review continuation.

### A10. Required bindings and non-class conversion need explicit examples

This continuation is internal documentation only. The codegen review now pairs `Iterator.iter()` and `Iterator.next()` signatures in Python, TS, Go and Rust. `iter()` cannot throw an application error, but returns an iterator whose later `next()` can throw its associated Error. Required-interface projection preserves the exact Item/Error bindings; it must not silently select Iterable's default Error=never. Registration checks required implementations together before publishing the host receiver.

Clarified that an interface pin does not identify a concrete data type to construct. An empty or nonempty native list needs an explicit typed-data conversion or host adapter at an interface input. The conversion uses BAML's existing array implementation; the adapter registers its own host receiver rather than replacing that implementation or registering every native list. Backend review must distinguish calling a BAML member from supplying an ordinary host implementation of that member.

Added required-binding and atomic-registration SDK cases and expanded the collection negative-case gate. Source review used TYPE_SYSTEM.md's implementation/coherence rules and the actual stdlib Iterable/Iterator declarations. The signatures remain proposed; no generated-code compilation or native tests were run in this documentation pass. Production changes already present in the checkout were preserved. The BEP and website remain unchanged.

### A11. Failure timing must distinguish binding from execution

Internal documentation review only; the BEP, website and production sources were not edited in this pass. Added a failure-stage matrix covering BAML compilation, SDK generation, native checking, binding, call admission and host completion. The three separate obligations are representing the declaration, checking the receiver and validating each invocation. Binding cannot prove arbitrary future native behavior and must never trial-run a user method.

A rejected host completion may follow owner-language side effects; those effects are not rolled back. This preserves the agreed owner-language mutation model without promising transactions or shared field mutation. Expanded existing planned SDK assertions to distinguish pre-body rejection from post-body completion failure and to check temporary ownership cleanup.

Added an explicit Error-only associated-binding negative case. Python/TS/Go implementation method shapes can omit E from their return/error syntax, but checked ref/input identities must retain it. Holding Output constant catches accidental erasure that an output-type mismatch would miss. Source review used TYPE_SYSTEM.md's associated-type and implementation rules and the stdlib Iterable/Iterator declarations. Generated signature compilation and native execution remain separate, open acceptance gates.

### A12. Connect the proposed signatures to generation and decoding

This continuation updates internal documents only. The BEP, website and production changes already present in the checkout are untouched. Added a source-to-signature walkthrough to the codegen review and design §9.2.2, with corresponding Python and shared-test plan gates.

Source inspection confirms that the shared interface graph preserves symbolic associated projections, declaration order and non-class implementation patterns. However, ordinary symbol conversion still maps associated projections to Unknown and removes injected client overrides; Python interface translation still emits Any, token classes have no methods, and result decoding has no interface-facade case. These are implementation gaps, not language limitations. The new graph cannot fix them without consumers and matching codecs.

The projection sequence now explicitly separates substitution from language rendering and native input acceptance from decoded output refs. A host method receives a checked ref when BAML supplies an interface argument; its completion may supply the matching input role, with registration/projection and ownership checks before BAML receives it. Apply this recursively to callbacks and records while respecting native container invariance. This prevents reusing a caller-input signature where a host body needs a callable ref.

Validation is source/document consistency and Markdown/link checks only. No generator compilation, native execution or new SDK implementation is claimed by this review. The proposed public helper names remain subject to the existing compile-checked example gates.

### F15. Carried-forward inbound dynamic-contract failure remains open

The preceding implementation continuation added exact inbound contracts for retained interface methods and strengthened `runtime_interface_inputs_use_exact_declarations_before_execution`. Its final recorded focused run fails when a valid retained Peer is supplied to the optional `Peer?` parameter: wire-only pre-coercion cannot select the interface arm before exact VM conversion. Direct, array, map and record assertions were reached successfully, but the complete test is not green. The earlier 56/56 engine result predates the final strengthening and cannot establish the current suite's status.

The required fix is to retain exact type evidence through union selection and recursive conversion, including values from distinct compilations with identical names. Reject invalid arguments before body execution and release partial adoptions. Do not solve this by trusting a displayed type name or accepting every raw interface value into every union. No code fix or test rerun was performed during the documentation-only feedback continuation.


F15 implementation follow-up: exact callable conversion now owns coercion and normalization. Direct retained values are checked by VM membership; aggregate children keep their exact expected types; an allocation-free matcher selects unannotated union shapes with live-child evidence before conversion. The expanded focused case passes. Broader checks and annotated/dynamic-alias paths remain to be verified; native bridge entrypoints/codegen and host-call completion contracts remain separate open gates. No BEP or website change.


### F16. Returning a host callable must preserve its checked callable identity

The new callback case first verified direct/nested runtime-created Named arguments during a BAML-owned Runner call. It then captured the host callable as a BAML return value and invoked that callable after major GC, outside Runner's package frame. This exposed two separate failures: outgoing HostClosure values were rejected as CannotConvert; after exporting them as ordinary retained callable handles, callback argument conversion failed by looking up the display name user.Named.

HostClosure now uses the same retained callable output lane as BAML closures and bound methods. Required/optional host-call parameter mapping keeps exact VM types through outbound conversion instead of projecting to names and resolving them back. The strengthened case passes, drains transferred CFFI interface leases, invokes the retained interface after the callback, and returns external roots to baseline with the engine still open. It exercises engine dispatch and the test wire dispatcher, not a generated Python/TS/Rust SDK.

This is the existing callback-like lifetime design applied consistently, not a new adapter or compatibility path. Full native returned-callable recognition, exact host result/error contracts across suspension, annotated inbound contracts, and the generated interface protocol remain open. Broader verification is recorded in the plan. No BEP or website change.

F15/F16 final verification: the combined engine selection passes 170/170 (one existing skip), and the engine/CFFI/ctypes all-target compilation and formatting checks pass. This closes the concrete failures described above, not the entire exact-contract or native SDK checkpoint. Commands and next gates are in the plan.

### F17. Host completions need retained contracts, too

The returned-callable probe exposed the next loss of type identity: callback binding rejected a runtime-created interface return as unvalidatable. The engine also projected callback return/error contracts to wire names before suspending, so a later completion could not reliably recover the original declaration after moving GC.

Root and child threads now trace and forward the current host call's exact return/error contracts. After reacquiring the heap permit, completion checks and conversion use those declarations. Binding accepts realized interface and `unknown` contracts; existing full-binding checks still reject unresolved generics. A returned or thrown interface view must satisfy the actual contract, including associated pins, regardless of its display name.

Copied records require strict field validation before materialization. Reusing permissive input coercion alone would turn an invalid integer result into a float. The new runtime-created Packet probe verifies valid fields, a wrong numeric field and a foreign interface child. The same boundary retains JSON's finite-number/data-only restrictions rather than treating its expanded union as the whole wire contract.

A further probe caught an overcorrection: checking every thrown payload against `throws never` also rejected a genuine SDK panic. Panics remain outside the application error set. The engine now identifies an actual retained panic type or a known builtin panic declaration and checks the payload against that separate schema. A malformed panic becomes a contract violation; a valid one propagates unchanged. This does not allow arbitrary host type names to bypass error checking.

Focused tests pass for the dynamic callback return/error cases across major GC on both root and spawned child threads, strict nested record fields, genuine `unknown`, valid/malformed panics and JSON finite/non-finite values. The combined engine selection passed 174/174 with one existing skip; the child-thread extension passed afterward. Compilation and final checks are recorded in the plan. These tests use the engine and test dispatcher; they do not establish generated SDK support or the unfinished registration, transactional lease and scope protocol. Sparse annotations, dynamic aliases and named generic inputs remain separate gates. No BEP or website change.

### F18. Named generic inference must retain declaration identity

Inspection of named function entry found that preserving types only during conversion is too late. Its shared inference solver accepted name-based types, while a separate first-binding walk recovered some live receiver types. That split could merge separately compiled declarations with identical names and omitted live interface children inside copied containers.

The existing inference solver now accepts declaration heads and an authoritative type context. The compiler's name-based helpers continue to use the same shared algorithm; named runtime calls supply VM facts and exact heads. Live interface values contribute their checked view, copied lists/maps retain live child types, and explicit bindings apply before inference. The resulting bindings drive both parameter conversion and the entry frame. Materialized arguments are checked before running the function body.

The new named-call probe passes for direct/optional interface pass-back, invariant arrays from the same versus different runtime compilations, explicit-type precedence and untouched receiver state after rejection. An extension also passes for an inferred associated output: a returned BAML `Source` carries `Output=Named` from its own runtime compilation, and a named generic function returns that exact interface value.

The first broader run exposed two production issues: type mismatch diagnostics lost the function and actual argument type, and an inferred opaque-host null was materialized as a plain VM null. Diagnostics now preserve that context; inline host data, including scalars/null, uses opaque storage when its selected contract is `rust_type`. A callback test also assumed the interface was union arm zero. It now verifies the selected index against the transmitted union schema; canonical order is not source order. The test's assertion caused an abort at its extern-C boundary, not an unhandled production fault.

Focused fixes and all 256 shared type/inference tests pass. The final combined engine selection passes 252/252 with two existing skips, and engine/CFFI/ctypes all-target compilation passes. Commands are recorded in the plan. This does not yet close sparse annotation, dynamic-alias or native SDK gates. The remaining wire-named method type arguments need the common protocol's exact type-reference lane; do not add another name-based reconstruction path. No BEP or website change.

### F19. Prepare calls before scheduling; distinguish live types from imports

The native adapters previously decoded and dispatched calls independently. Python, Node, Java, C and Wasm now share preparation and invocation. Preparation retains the callable or checked interface receiver and any live type arguments before handing work to an executor. Releasing an SDK reference after this point must not invalidate admitted work. The retained engine checks runtime provenance again at invocation.

Type arguments now have three explicit forms: a static named type, a portable definition to import, or a live reflected-type reference. Exactly one must be supplied per binding. Named function bindings have unique nonempty names; a checked method's bindings are ordered by its declaration. The engine resolves live evidence under its heap permit, retains exact declaration identity through specialization, and checks method arguments by the interface's parameter names and optional modes. A concrete implementation may rename required parameters or add/reorder optional parameters without changing that caller contract.

A live reflected type returned through the session is now an owned type handle. Explicit artifact export still writes portable definitions. A foreign live type is rejected both as type evidence and as an ordinary argument; the old value-conversion fallback that silently imported its definitions has been removed. This makes reference identity consistent rather than depending on which argument lane was used.

This is an implementation checkpoint, not the completed ABI cutover. The current schema still lives under v1 and the existing fragmented handle table remains. Ordinary value arguments still use its transfer/drain behavior; new receiver/type-evidence fields borrow and pin references. Transactional partial-failure cleanup, role/session/generation consolidation, mandatory ABI/schema negotiation, all generated SDK facades and scope draining remain open. Encoded CFFI tests and native adapter compilation are being verified; they do not establish generated-client support.

F19 verification follow-up: all 105 shared bridge tests and 252 affected engine tests pass (two existing engine skips). Encoded method calls preserve a runtime-created associated Output; two compilations with identical type names remain distinct. Correct calls survive release of their original type/value references, wrong bindings leave receiver state unchanged, and dropping a prepared call releases its checked view without runtime shutdown. Native adapter all-target checks and Wasm compilation pass. Exact commands and remaining work are in the plan. This does not establish generated SDK or complete lifetime-protocol support.

### F20. A value walk is not an ownership boundary

The prior decoder consumed handles only when it reached them. An invalid annotation, malformed early child or invalid invocation target could leave later transferred handles in the table. The encoder had the inverse problem: successfully encoding one child committed its handle before a later child failed. Callback arguments repeated that mistake across separate arguments.

Input decoding now adopts the whole parsed aggregate into one owning batch before semantic validation. Decoded values take ordinary strong references from that batch; dropping it releases anything not retained by a successful decode. Each engine-handle occurrence consumes its transferred lease, while the caller's separately owned original survives. Host registrations are retained by their existing interned Arcs. Conflicting host kinds now produce an error rather than a debug panic, and handle inputs no longer bypass their sparse type annotation.

Output graph encoding uses one insertion ledger for the whole result or callback argument list. It rolls back all table insertions if any child fails or the encoder unwinds. This is the engine-side graph portion of the intended ownership contract. It does not yet solve delivery/adoption across the transport: an undecodable envelope needs ownership metadata outside its value payload, and a cancelled or partially decoded outbound message needs a receiver-side adoption transaction. Raw host-key outputs also need retained ownership through that handoff. Keep these as required protocol work, not compatibility exceptions. Focused validation is in progress; see the plan.

F20 follow-up: rejected preparation now drains host releases at the native safepoint because it never reaches the engine's usual drain. Callback completion also adopts its parsed payload before validating the completion flag; an invalid flag releases the payload rather than leaking it. The final shared run passes 120/120, and the preceding affected callback/interface engine run passes 58/58 with one existing skip. Native adapter and Wasm compilation pass. This verifies engine-side graph cleanup, not the still-open transport handoff or generated SDK APIs.

Design §8.1 now states the remaining transport mechanism explicitly: an ownership receipt outside the value payload, retained staging ownership, provisional SDK wrappers, atomic adoption of claimed leases, discard of unclaimed/late results and teardown of abandoned stages. Inspection also found that an empty InboundValue encodes null, while the existing callback error flag treats empty bytes as a missing throw payload. The replacement completion envelope must represent present null separately. This is required new-protocol work, not a request to preserve that old limitation.

### F21. Encoded bytes need an owner until the SDK adopts them

Added the shared receipt implementation and routed result classification through an owned encoder. `EncodedTransfer` retains all leases and raw host-registration Arcs; `TransferSession` stages them, validates session-qualified receipts and claim multiplicities, and removes completed transfers. `PendingDelivery` discards when its sender abandons delivery. A receipt stays outside the payload, so malformed bytes do not prevent cleanup. `invoke_prepared_encoded` exercises this ownership with real BAML interface calls; callback argument aggregates have an owned encoding entrypoint too.

Each encode attempt has its own temporary ledger. If an invalid child requires an SdkPanic fallback, the failed root's owners are released before that fallback is staged. Successful decoding can claim a subset of leases: cache hits need not adopt redundant ownership. Host registrations stay alive until the SDK has had a chance to recover the original host object. None of these paths requires a singleton runtime to retain completed transfer history.

Cleanup exposed a related locking issue: the handle table previously destroyed its last RustData/ADT owner while holding its write lock. Destruction now runs outside that lock, as well as outside the transfer-session lock. Each table lease has its own drop guard so a panicking resource destructor does not orphan later siblings. Adoption keeps claimed leases provisional until unclaimed cleanup succeeds; cleanup panic rolls them back instead of leaving keys that no SDK wrapper owns. Native ABI entrypoints still need their normal panic boundary and host-release safepoint.

This establishes shared ownership machinery, not the complete transport contract. Existing adapters still explicitly commit through `into_unreceipted`; remove that temporary entrypoint when their native result types and decoders switch together. Runtime/session teardown wiring, provisional native wrappers, callback completion receipts, native release draining, and the mandatory ABI/schema handshake remain required. No legacy delivery behavior is part of the accepted final design. The plan records verification and remaining ports; the BEP and website are unchanged.

### F22. Python result delivery must own cleanup through model validation

Python's function-call path now returns an owned native `BamlEncodedResult`. Its receipt stays outside the encoded payload and is retained through PyO3 future delivery. Dropping an unread result discards it. Decoding creates provisional handles and keeps a strong root set until the entire result has passed validation. A validator may replace a decoded child or retain it while raising; neither behavior can make a receipt-owned key acquire an independent premature release. Failed handles are invalidated, and their shared runtime retention is cleared even if the validator kept the wrapper.

Successful decode adopts before returning **or raising the decoded BAML error**. A valid error outcome is different from a decoder exception: its nested references must remain callable. Original host exceptions are recovered before adoption releases their registration. A nested ContextVar transaction keeps a reentrant BAML call made by a validator separate from the outer decode. The singleton runtime slot now snapshots engine and transfer session together; replacement and shutdown close the old session outside the global slot lock.

This changes the Python native function-result type, not ordinary generated function return values. Native callback arguments, unhandled errors and introspection still use raw-byte paths until their corresponding ports. That temporary split is internal implementation staging and must not survive the final ABI cutover. The new tests exercise the actual Python extension, including Pydantic and async delivery; results and remaining verification are recorded in the plan. BEP and website remain untouched.

F22 validation exposed failure decoding that depended on generated builtin models. With an empty typemap, an InvalidArgument or SdkPanic became an unrelated unknown-class error, and a same-host exception lost its identity. Python now uses SDK-owned BamlFailureValue payloads for builtin failures, preserving class names, fields and pass-back encoding without application model lookup. Application-defined failures retain normal generated validation. Shared classification now treats a missing function as InvalidArgument. Tests expecting an ordinary exception for an invalid host result now assert the actual HostContractViolation panic; permitted native exceptions assert original-object identity. This follows the accepted error categories instead of preserving the old masking behavior.

The first broad shared run also found a stale generated C header missing the interface-handle discriminator already present in Rust. Regeneration through nextest adds that one enum value. This is header consistency during implementation, not the still-required final ABI/schema cutover. Native verification and exact results are recorded in the plan.

### F23. A returned host value needs ordinary owned-reference semantics

F22 retained raw host registrations until decoding, which is enough to recover an original exception immediately. It is not enough for an opaque host value retained as part of a decoded result: adoption released the registration's last owner while leaving a bare SDK key. The fix uses the existing handle table for returned HostValueArc owners, with a distinct HostReference discriminator. Initial registration keys still name the host registry; returned reference keys name owned table entries. They must never be interchanged, even if their integer values happen to match.

This removes the separate outbound host-Arc ledger. One receipt now owns all engine and host reference leases, so partial failure, adoption, copying, unclaimed-child cleanup and pass-back use the same mechanism. Python resolves an owned host reference to the original registration for exception rehydration. Last-handle release drains host notifications outside the table lock, including notifications queued by dropping the last SDK-owned runtime state. The C release boundary contains resource-destructor panics.

This is an implementation migration, not an ABI compatibility promise. The schema still needs its final version/fingerprint cutover, and other bridges' host-exception lookup paths still need to consume owned references rather than interpret table keys as registration keys. New shared and native Python tests cover adoption, copy/pass-back, decode failure and cleanup with no later VM call. Their results and remaining ports are recorded in the plan.

### F24. Packed Git refs caused repeated compiler rebuilds

F23 verification repeatedly rebuilt the compiler even for narrow unchanged checks. Inspection found that baml_artifact's build script watched the current branch's nonexistent loose ref file; this checkout stores that ref in packed-refs. Cargo treats a missing watched path as dirty on every invocation. The watcher now tracks an existing ancestor for creation of a loose ref and the packed-refs file when present. The build fingerprint still comes from the exact Git HEAD.

A standalone probe of the real build script in a temporary Git repository verifies loose refs, packed refs and a later commit that creates a loose ref again: every watched path exists and the fingerprint matches HEAD in all three cases. Two consecutive artifact checks verify that the unchanged second build reuses its cache. This is a build-loop correction discovered while implementing the reference protocol; it does not change the protocol's type/declaration identity rule. Exact verification is in the plan.


### F25. Node media wrappers must share provisional ownership

A media wrapper cannot clone a table lease out of a result that has not been adopted: a later model-construction failure must invalidate every escaped view. Node now roots native lease owners in its encoded result and shares those owners with typed media wrappers. This avoids retaining JS wrapper objects through N-API references while preserving one adoption decision for the complete aggregate. Active explicit clones acquire independent table leases and retain the originating runtime state.

Valid decoded errors are adopted before delivery. A failure while constructing an error value instead discards the result, just like a failure while constructing a success value. The former Node decoder swallowed that error; it now propagates it. The function-result port passes 123 distinct Node/native tests plus native and TypeScript checks (plan F25); callback and other raw-result paths are still migration work, not a compatibility guarantee for the final ABI.

### F26. Callback delivery needs ownership before scheduling, including N-API failure paths

The preceding F25 turn made verified implementation progress. The next port replaces the native callback producer's early ownership commitment with `encode_to_host_call` and an owned Rust dispatch entry point. A missing dispatcher or rejected native handoff now drops the argument aggregate. Node and Python stage callback arguments in delivery-owned receipt sessions and adopt the complete argument list before entering a user body. These delivery sessions do not select a new global runtime or claim to implement scope revocation, executor shutdown, or tracked-work draining. Full issuing-runtime identity remains part of the reference/invocation work; the delivery owns the exported value leases independently of the caller's cancellation.

An unexpected transport issue was found in the installed napi 3.9.0 source (`threadsafe_function.rs`): `call` hands a Box to N-API without recovering it on enqueue failure, and `call_js_cb` returns on a null environment before recovering the queued Box. Moving owned results into that helper would leak their receipts. The Node bridge therefore uses a small owned dispatch queue: rejected enqueueing reclaims the message, null-environment delivery frees it without invoking JS, and native conversion/handler failures discard an unadopted message even if JS retained its result wrapper. Already adopted ownership is never revoked by a later handler failure. The queue's delivery callback catches cleanup panics at its C boundary.

This follows the [Node-API threadsafe-function contract](https://nodejs.org/api/n-api.html#asynchronous-thread-safe-function-calls), which requires queue-data cleanup when JS execution is no longer possible. The implementation also stops using a queue pointer after a Closing result and serializes enqueueing with finalization. Tests use the actual queue and resource Weak references to measure rejected messages, a direct null-environment call to the production delivery callback, and JS handlers that throw before or after adoption. This is not yet a full multi-environment/worker-termination acceptance test.

The existing separate Node host-value **release-notification** queue still drops notifications on saturation. A later GC cannot repair those missing notifications; misleading comments were corrected. That queue needs a lossless release transport before lifetime work is complete. Other C/native consumers still use the transitional byte callback route and must migrate before the final ABI cutover. No BEP or website changes.


F26 verification is recorded in the plan: 129 distinct Node cases, 98 Python passes (plus existing expected/unexpected markers), and 14 shared dispatch cases pass. Node/Python builds and TypeScript compilation pass. Four-format callback payload tests succeed in both languages. These bridge-local tests prove these transport paths; they do not substitute for generated `sdk_tests` execution or the remaining lifetime/ABI gates.

### F27. A release notification must survive scheduling pressure

The old Node release channel queued one message per key and dropped a release when its bounded queue filled. That permanently retained the corresponding JS registry entry: another GC could not recreate a notification already consumed by Rust. Pending release keys now stay in a native set until the JS registry finishes deleting a batch. One wakeup drains up to 1,024 keys, then schedules the next batch if necessary. Enqueues during delivery join the set; a partial JS failure leaves the batch pending for an idempotent retry. Empty pending storage is freed, including after repeated bursts with the singleton still open.

The release channel is unreferenced, so it does not itself keep Node running. Native environment finalization frees undeliverable pending-key storage; ordinary reference disposal does not destroy the JS environment. Enqueueing uses the same direct N-API approach as F26, with no per-message allocation to lose on rejection. Unexpected native enqueue errors preserve pending keys and log the failure; a later enqueue can retry. This is not a promise of recovery from arbitrary transport failure.

Native tests exercise real final table-owner releases into the production JS registry, plus isolated retry, reentrant enqueue, duplicate-key, full-width-key and close cases. A subprocess verifies exit without explicit runtime shutdown. Verification is recorded in the plan. These results cover release delivery after Rust determines an owner is dead; quiescent VM collection, cross-runtime cycles and worker-specific registration routing remain separate required work. In particular, the existing process-global first-registration-wins slot is not safe routing for independent module instances or Node worker environments. Removed the misleading comment claiming arbitrary reloads are harmless. The BEP and website remain unchanged.

### F28. Unhandled spawned errors carry ownership, including during shutdown

Unhandled-error reporting was still a bare-byte path. A thrown value can contain live interface, callable, media or host references, so this path needs the same ownership boundary as an ordinary result. The shared notification registry now owns encoded aggregates while waiting for callback registration and transfers them to native handlers. Python and Node stage an independent delivery receipt, decode provisionally and adopt before reporting the decoded error. Node reuses the owned dispatch queue with an unreferenced, unbounded notification channel; it no longer blocks while scheduling error notifications. Failure and environment teardown release undelivered ownership. The C byte route remains a temporary porting boundary to remove before the final ABI cutover.

A reporter can retain the native envelope and then fail. That must invalidate unadopted ownership, while preserving refs it already adopted. The native tests use a real spawned BAML error containing a host object, exercising unread delivery, successful error adoption, decoder rejection and broken reporting. Dropping a receipt remains independent of the runtime's global slot, including after shutdown. These tests do not establish full worker routing, issuing-runtime reference identities or scope draining.

The stricter Python reporter test exposed two different kinds of retention. Its diagnostic traceback initially retained the test helper's original Python object; removing that local reference isolated the bridge behavior. With zero live table handles and no Python referrers, a dead registration still remained until a later SDK operation drained releases. The engine kept an original error copy while invoking the reporter, so the reporter's own discard/drain happened before the final HostValueArc dropped. The engine now drains releases after reporting, and bridge shutdown drains after dropping its runtime owner. An engine test checks this after collection while the engine remains open; the Python test checks reclamation before any subsequent SDK operation.

The engine test initially assumed one collection could both materialize an object-valued error and reclaim its VM storage. Inspection showed that the reporting collection deliberately roots that object until materialization finishes; a subsequent collection reclaims the now-unrooted storage. The corrected test asserts one report, then reclamation and host release after the subsequent collection, with no runtime shutdown driving it. This verifies eventual collection and draining, not automatic idle scheduling; that separate requirement remains open.

The collector inspection also found that its current source is a no-op shim: no collector logs are produced. Its old output codec remains an explicit cleanup/port item; this change does not claim to have exercised collector introspection. Verification and remaining work are in plan F28. The BEP and website remain unchanged.

### F29. Python interface methods must retain their issuer as well as their receiver

Implemented the native/Python ref and decoder path required by A12. Interface results now become owned `BamlInterfaceRef` values, with optional generated-subclass registration in a separate typemap. `_invoke` prepares the checked interface target and argument ownership before scheduling, using the runtime and transfer session retained by the result's adopted handle. It never routes a retained interface through a later singleton. A closed/replaced issuer rejects new calls; an admitted call pins its receiver independently of local `close()` or wrapper collection. Concurrent teardown still relies on result-session rejection and the broader pending cancellation/drain work; this does not establish full shutdown linearization.

Each ref retains its decoding typemap for nested method results and errors. Copies acquire independent leases on the same receiver; `close()` releases only that wrapper. Decoder failures invalidate provisional refs and release their receipt. Method errors adopt their nested refs before raising. A scheduling-failure test also verifies callback argument registrations are released immediately when no Python event loop exists; a native exit guard drains releases after future conversion fails.

Native tests reuse the shared BAML fixture and cover class/default methods, non-class array/string receivers, nullable iterator items, exact associated Error pins, explicit generic specialization, pass-back, owner-language mutation, copies, decoder rejection and runtime replacement. Small handwritten subclasses exercise the registration boundary; **this is not generated-SDK validation**. The generator still needs typed ref/input/host roles and methods. Callback and unhandled-error deliveries currently retain no issuing runtime, so interfaces arriving through those paths cannot yet invoke methods. Resolving their issuer at dispatch and carrying it through delivery is required; a late global lookup is not an acceptable substitute. Other bridges, media/provider integration and the final protocol remain in scope. The BEP/site are untouched; verification is recorded in plan F29.

### F30. SDK generation needs the compiler's interface-method callability decision

Generating a method signature is not enough to know whether that method belongs on an interface reference. Some methods can dispatch through an interface reference; others require a known concrete `Self`. For example, two values implementing the same interface are not necessarily the same concrete type, so a method taking another `Self` cannot accept an arbitrary second interface reference. Receiverless methods also need a concrete implementation target.

This is an existing language rule, not a new restriction introduced by this PR; see [TYPE_SYSTEM.md](TYPE_SYSTEM.md#self). The compiler already determines existential callability for method resolution, and runtime method metadata carries that decision. The shared SDK export must carry it too, so Python, TypeScript, Rust and the other generators do not independently reconstruct the rule from method signatures. Methods requiring concrete `Self` must not be exposed as ordinary callable methods on an existential ref. Their implementation-side declarations and any supported concrete-type API are separate surfaces.

The export should preserve the restriction reason for useful diagnostics. Verification should cover callable methods, an additional `Self` parameter, invariant nested `Self` results, and receiverless methods, including precompiled packages. This entry records the discovered export requirement; it does not claim that all generators or generated SDK tests implement it yet. No change to TYPE_SYSTEM.md or the BEP is required to establish the existing rule.

F30 implementation now carries the compiler-derived callability category and uses it in Python caller generation. Required/default existential methods have generated async wrappers; concrete-Self and receiverless methods are not falsely exposed as existential calls. Full restriction diagnostics and the corresponding concrete APIs remain open. Python's type parameters preserve associated declaration order and invariant Error bindings. Both generated token forms encode associated pins separately from ordinary generic arguments. Method `_types` validates the exact key set and preserves definition-carrying type tokens for the existing checked invocation path; this explicit spelling is an intermediate implementation surface, not the final inference ergonomics.

The generated shared fixture exposed a separate representation issue: `FriendlyGreeter.new_async()` still returns a Pydantic copy. It cannot supply a live interface receiver, and the shared conformance check correctly rejects it. This confirms the need for design §11.7's concrete facade projection; it does not justify structural admission, `.as_client()`, or making every class/array live. The engine currently distinguishes dynamic runtime objects and a few trusted stdlib capabilities, while static authored class instances normally serialize their fields. That static-versus-dynamic distinction alone cannot implement the agreed data-versus-behavior rule. Add shared projection metadata before changing the codecs so bytecode and every generator agree, including source-less packages, implementation defaults, out-of-body rules and portable builtins.

**Future BEP update: copied records versus live concrete classes.** Explain the proposed boundary rule with paired examples: a plain data class becomes a copied host record; a class with exported authored instance behavior becomes a wrapper for its original BAML receiver. Interface implementation/default methods count as behavior; static factories, empty markers and compiler-generated serialization alone do not. Dedicated builtin data codecs take precedence. A copied outer record can contain a live child without becoming live itself. Factory results must preserve the receiver so it can be passed directly to an interface parameter, and methods may update BAML-owned state without exposing synchronized writable host fields.

Explicitly call out the API consequence for review: under this heuristic, adding an exported instance method to a data class can change its generated SDK representation from a copied model to a live wrapper. This is a bridge API policy, not a requirement of TYPE_SYSTEM.md. The BEP should explain that consequence and record the final choice before presenting the heuristic as settled public behavior. This note records the follow-up; it does not update the BEP or claim the concrete-class path is complete.

F30's actual generated SDK passes four shared native cases, with the concrete factory case still enabled and failing. Static consumers and supplemental generic/non-class probes now exercise generated code instead of handwritten facades. General `T.Output` projections currently produce a member-specific generation failure rather than Any; the checked specialization path for those legal declarations is still required. Full native input/host roles, inheritance, client overrides, callback issuer propagation and all-language support remain open. Exact verification and next steps are in plan F30. The BEP and website are unchanged.

### F31. The class's defining package owns its default bridge projection

An implementation can legally live in the package that defines an interface, even when its receiver class comes from another package. If every such extension changed the class's default bridge representation, the same precompiled class would cross as a record in one application and a live object in another. Its existing generated SDK could not agree with both. Blanket implementations would create the same problem for otherwise unrelated classes.

The defining package therefore chooses a class's default projection. Its authored inherent methods and class-headed implementation rules count, including out-of-body implementations and inherited defaults from dependency interfaces. Downstream and blanket implementations remain available through checked interface views, without rewriting the concrete default codec. The choice is per class declaration, so a conditional rule can make a generic class live while the rule's methods/conversions remain conditional on its actual type arguments. This is a bridge representation rule, not a change to BAML conformance, coherence or dispatch. It needs no user-visible `.as_client()` step for generated concrete implementors.

The compiler now computes this decision once. Package interfaces retain it with authored-method provenance; bytecode class declarations and the shared SDK SymbolPool carry the same projection. The alias registry identifies compiler carriers, whose dedicated codecs take precedence. Static factories, empty/field-only interfaces, generated serialization and copied outer records with live children do not become authored live classes. Source-less package consumers use the serialized owner decision. Runtime mount linking stubs also defer to that decision; a field-only stub cannot erase behavior from the actual class. Artifact format 4 rejects the old Borsh layout; rebuilding the CLI, stdlib artifacts and native bridges together is required before running newly generated bytecode.

This metadata is the prerequisite for the concrete-object codec/facade cutover. F31 does not yet change outbound encoding or make the FriendlyGreeter factory case pass. The runtime and generated codecs must consume the metadata together; existing dynamic-value and stream tags are not substitutes for an exact concrete-object reference role. Verification is recorded in plan F31. The full one-PR scope and the BEP/site hold remain intact.

### F32. Concrete transport is distinct from generated facade selection

The outbound runtime now retains live concrete class receivers under their own role, including runtime-created classes. Python concrete and interface wrappers share ownership operations. Passing a concrete receiver to an interface parameter uses the existing runtime obligation check on that actual object; no user conversion step or copied-field reconstruction is needed. Explicit data conversion uses the receiver's issuing runtime and rejects a closed/replaced issuer. Runtime-created enums still use the older runtime-value role in this intermediate implementation.

This exposed a separate codegen requirement: receiving a concrete handle does not prove that its diagnostic class name denotes a generated declaration. A runtime-created class can reuse that name. The interim Python decoder therefore returns a generic concrete reference and cannot yet promise the public API or annotations of `ResponsesClient` or another generated facade. The final selection must use checked declaration/package metadata and actual class arguments. Implemented/default methods also need their proper compiler-owned targets, not an assumed `Class.method` name. The technical design's §11.7 records this distinction.

Keep runtime pass-back, typed facade generation and typed input acceptance as separate acceptance checks. In particular, a shared factory test passing dynamically does not establish correct generated Python types. Test the generated concrete return type and direct interface input under a native type checker as well. This does not change the desired user experience, add `.as_client()`, restore field mutation, or reduce the all-interface/all-bridge scope. Verification is recorded as it completes in plan F32; the BEP/site remain unchanged.

### F33. Concrete calls need their own compiler-resolved contracts

The concrete facade cannot be generated by copying its interface methods. BAML permits an implementation to refine its signature, and a default method may belong to a different generic frame. The shared graph now exports the compiler-selected concrete signature, its inherent or interface dispatch target, method parameters and bounds, and ambiguous names. It preserves top-level argument names for keyword calls and uses distinct identities for class and method type parameters. This is metadata for the forthcoming native caller, not a change to the language's dispatch rules.

Keep that signature separate from complete implemented-interface views. For example, `read() -> string throws never` can implement an interface whose actual `Error` binding remains `string`. The input role must retain `Error=string`; inferring `never` from the method would invent a different interface type. Reuse the compiler's complete-view resolver, including block-level associated bindings and overridden defaults, and its parameter-environment checks. Only universally applicable views/methods belong on an unspecialized class facade; keep other implementation rules for checked specialization. Required interfaces and empty markers remain facts even when they add no method.

Source and mounted packages may enumerate implementations in different orders. Sort exported method names, ambiguity sources and implemented views so SDK output does not depend on that order. The source-less comparison must still compare full signatures, targets, frames and pins; ordering cleanup is not permission to omit semantic checks.

This does not finish generated concrete methods or direct typed inputs. F32's concrete Pyright acceptance probe remains an open gate, as does facade selection using checked declaration/schema evidence. Existing static type tags are name-derived, not schema fingerprints; dynamic declarations with the same diagnostic name must still decode as their own exact references. Technical design §11.7 now distinguishes concrete and existential caller contracts. No new field mutation, mandatory scope, `.as_client()`, or BEP/site update is introduced. Verification is recorded in plan F33.

### F34. Pair concrete facade selection with the loaded SDK artifact

A generated SDK already includes the bytecode it describes. The validated loader can therefore establish exact-bundle agreement without hashing display names or running a BAML call during decoding. The shared artifact crate defines a domain-separated bundle digest. The loader attaches it to the engine, and outbound conversion captures static concrete declaration evidence under its existing permit. Dynamic declarations receive no static evidence. This adds runtime metadata without changing the serialized Program layout.

Python's native table lookup checks that evidence before consulting the typemap's separate concrete-facade entries. A stale bundle fails decoding; an unavailable/dynamic declaration keeps the generic owned reference. The lookup may read immutable metadata while a result is provisional, but may not invoke methods, grant ownership or revive a discarded result. Normal aggregate adoption and failure cleanup still apply. The same checked metadata must be exposed through the remaining bridges; a Python-only helper is not all-language parity.

Exact bundle agreement is a sufficient first pairing path, not a declaration compatibility algorithm. Two runtimes loaded from the same artifact may use the same generated definitions, while each reference still routes to its own issuer. A different program with a compatible package needs the planned declaration-level negotiation; source compilation currently supplies no bundle evidence. Keep that gate open rather than treating exact-bundle rejection as proof the schemas differ. General interface-reference selection also still needs its final declaration checks.

Generated method bodies, native input roles and concrete configuration construction remain separate requirements. Generated functions and factories must also validate their SDK/runtime binding before execution; a decode-time mismatch happens after the body and is not that admission check. The identity probe deliberately uses small explicit subclasses against a real generated artifact; passing it proves selection and ownership behavior, not the complete generated concrete API. See plan F34 for verification. The BEP/site remain unchanged.

### F35. An SDK binding selects a runtime; it does not own its closed heap

Python's old runtime wrapper selected the current global engine on every call. That allowed a retained generated function to run against a replacement before result decoding detected a mismatch. Generated free functions and method factories now capture their SDK's runtime and typemap. The native runtime object binds one transfer session and a weak engine reference. It consumes argument transfers on rejection, rejects a closed/replaced session before execution, and retains the engine only after admission. Direct async cancellation targets that bound engine.

This answers the singleton lifetime question more precisely: a loaded SDK module may live for the process lifetime without keeping a closed engine's receiver heap alive. Receiver references and admitted calls have their own ownership. The default engine can remain global, but its old object registrations must still be released when their last ownership disappears. Tests distinguish retained SDK bindings from live object references; they do not prove general cross-language cycle collection.

Encoding and decoding must use the same SDK map. Python now scopes argument reverse lookups and generic type tokens, restores the previous context even on encoding failure, and carries the map into result decoding across `await`. Replacing the process default cannot change those calls' codecs. Actual generated root imports and static factories were probed against a rebuilt SDK, including identical-bytecode replacement and callback argument cleanup.

Two independent issues surfaced during testing. Python callbacks currently run on a separate event loop/thread, so an `asyncio.Event` created on the caller's loop is not a valid executor test barrier. A thread-safe barrier isolates result-codec selection; caller-loop callback scheduling remains required. Also, a Python callback returning `None` to a BAML `void` contract was rejected as a host contract violation. The binding fixtures now use integer acknowledgements. Fix and test null/void normalization separately across callback and interface result paths; the passing binding tests do not establish that result mapping.

This is the installed/default runtime path, not a completed independent runtime-owner API or full declaration negotiation. Checking session closure before admission is not an atomic shutdown/drain protocol. Context-manager cancellation, callback issuer/codec propagation, streams, function specifications, runtime values and other language ports remain open. Concrete facade methods and typed interface inputs also remain unfinished. See plan F35 for verification. The BEP and website remain unchanged.

### F36. Concrete method contracts must survive invocation

F33's generated concrete signature can differ from its interface declaration, so routing every concrete method through the existing existential operation would make those annotations inaccurate. For example, an implementation accepting `unknown` can satisfy a `string` input contract; a concrete caller should retain that broader input. It may also have a different required parameter name, extra optional arguments, a more specific result or a narrower error effect.

The engine now has an explicit concrete-implementation operation alongside existential invocation. Both use the same checked receiver, implementation world, complete associated bindings, implementation/default lookup and generic-bound validation. The concrete path keeps the selected callee's contract and actual `Self`; the existential path keeps its declaration contract and restrictions on concrete-Self operations. The owned bound callable retains its receiver and realized frame after the original reference is released.

A generated class call also needs to prove which concrete declaration it describes. Its engine entry resolves the expected class and compares the actual declaration pointer against the receiver's class. The compiler's interface obligation can contain class slots; those slots are filled from the retained instance, not supplied as method type arguments or copied from a host annotation. Generalized the shared type rewrite over declaration-head representations so substitution preserves live type identity. The resulting obligation must be complete and implemented by that receiver. Wrong slots, pins and classes are rejected before execution.

This is shared runtime support for the forthcoming bridge target and generated concrete methods. It does not by itself fix the concrete Pyright acceptance probe, emit facade/input types, provide interface field access, or establish all-language support. Codegen still omits ambiguous/unproved unconditional methods according to the compiler graph; explicit qualification remains a separate checked choice. The named class selector serves known generated static declarations, not arbitrary dynamic declaration compatibility. See plan F36 for verification; BEP and website remain unchanged.

### F37. Use the common owned call path for concrete methods

The bridge now has an explicit concrete-interface-method target. It carries a borrowed concrete receiver key, the expected static class, the compiler-selected interface pattern and separate positional method type arguments. Preparation pins the actual ConcreteObject entry before yielding. It does not infer the receiver role from a diagnostic wire type or turn an interface reference into a concrete class reference. The engine supplies the stored class arguments and validates the full obligation as described in F36.

Python can reuse its existing issuing-runtime owned-call helper for this request. A second native invocation/lifetime subsystem is unnecessary. The encoder builds the target inside its existing argument-ownership rollback boundary, including method type evidence, so a later encoding error releases earlier registrations and cloned leases. The concrete wrapper scopes encoding to its receiver's typemap and decodes using that same map. Native preparation, result receipts, rejection and failed-scheduling cleanup remain shared.

The wire tests cover concrete parameter names/defaults, wrong class and wrong-role rejection, retained receivers after SDK-key release, and retained method type evidence/live arguments. These are shared protocol checks; only the Python helper is connected at this checkpoint. Schema generation is not native API parity. Typed concrete facade/method emission and input roles, inherent-method support and the other SDK helpers remain required. See plan F37 for the final verification results. BEP and website remain unchanged.

Native Python validation distinguishes argument membership failures from declared BAML throws: wrong concrete declarations and unresolved class slots become `TypeError` through the existing decoder, while closed-issuer failures follow the SDK boundary-error path. The tests assert those actual categories. They also verify that a record containing a live child can pass through a generic concrete method after the default typemap changes, using the receiver's map for both argument encoding and result decoding. These tests call the internal helper directly; they do not stand in for generated facade signatures.

### Scope review: consider deferring general SDK mappings for bounded generics

The side-conversation review identified a possible scope reduction: defer general host-language APIs for bounded generic functions and classes, such as `add_to_self<T extends baml.ops.Add<T>>(item: T) -> T.Output` and `OrderedSet<T extends baml.ops.Compare>`. These are part of the original all-interface ambition, but their full per-language mapping can be delivered later. This is a proposal recorded for review, not a blanket removal of existing implementation gates.

The proposed first release retains interface-existential references with fully determined associated types, BAML-defined concrete implementations, client overrides and Agent use, plus the generic operations needed for those promised examples. Before accepting the deferral, audit the actual client/Agent signatures and generated dependency graph: a construct required by those examples cannot simply be deferred while still claiming the examples work. Deferring SDK mappings does not remove bounded generics from BAML or weaken engine conformance checks.

Preserve bounds, concrete type identity and associated-type relationships in shared compiler/export metadata now. Unsupported SDK projections must produce explicit generation diagnostics, never silently disappear or erase constraints to `Any`/`unknown`. Define how an unsupported declaration affects generation of the surrounding package; a diagnostic alone is not a usable deferral if it blocks the required client SDK. Later work should specify exact native signatures, specialization/type evidence, dependent results, invariant class arguments and failure timing for each backend, with positive and negative tests.

For a future BEP revision, replace an unrestricted “all interface usage across bridges” claim with the exact supported subset and deferred cases if this scope cut is adopted. The current note changes neither the BEP/website nor implementation-plan acceptance gates.

### F38. One concrete target, two checked method selectors

The common operation introduced in F37 was named for interface implementations, but generated concrete classes also expose inherent methods. Rename it to `ConcreteMethodTarget` and make the dispatch choice explicit. An inherent call checks the expected class, finds its receiver method, retains the actual receiver’s class arguments, and appends checked method arguments. An interface-selected concrete call keeps the selected implementation/default contract described in F36/F37. Both share transfer preparation, issuer retention and outcome handling. No compatibility alias is needed for this unpublished ABI.

This is internal plumbing. Users still call `await object.method(...)`; they do not choose a dispatch selector or supply a receiver’s class arguments. These methods may mutate state in BAML. They do not expose writable host fields. The focused engine/common-wire suite passes 18/18, and the rebuilt Python concrete-reference suite passes 20/20. These prove the internal call path; generated concrete facade emission and typed input roles remain pending. See plan F38 for commands and timings.

### F39. Generate concrete methods from the compiler contract

Python concrete facades now consume the separate concrete caller graph rather than treating all classes as Pydantic records. Implementation/default methods and inherent methods use the same retained concrete call target with their respective selectors. Their own method type arguments are explicit until inference is implemented on this checked bind path; annotations must not advertise inference that the runtime cannot perform. The generated wrapper prepares the call before returning its coroutine, preserving the admitted receiver if the caller closes its SDK reference before awaiting.

The record typemap and live-facade typemap remain separate. Native type annotations inside generic records may refer to a live facade, but annotation lookup cannot construct one from a diagnostic name. Value decoding still requires the checked native declaration/bundle evidence. Source-only SDK initialization and extraction of a pure shared wire encoder remain open integration issues; see plan F39. The generated runtime/generic typing paths are now verified, while the exact interface input role is still pending. The concrete acceptance probe retains one diagnostic for that missing input role.

The generated probes also required completing each method target’s associated pins in the shared compiler export and emitting interface Ref definitions before eager dependent aliases. This removes the `ai.ClientRef` import cycle without a provider-specific workaround. Generic concrete type tokens now preserve their origin and class arguments independently of Pydantic. Real `ResponsesClient` construction/id and direct input to `Agent.new_async` pass offline; this does not verify `Agent.run`, LLM override arguments or streaming. Exact commands and remaining gates are in plan F39.


### F40. Separate native input evidence from concrete-data admission

The generated Python probe supports private invariant witness types with per-view overloads: one live facade satisfies two instantiations of one interface, while a mismatch in associated Error alone is rejected. This avoids asking users to inherit a generic base twice or provide a fake method to carry the Error parameter. The private generated methods serve native typing only; copying them onto an unrelated object does not grant runtime conformance. Input roles and ordinary caller methods remain distinct.

A new runtime failure makes the other half of this contract explicit. A generated copied record implementing a marker interface passes native typing but is rejected during argument admission. Keep the intended API: the SDK knows that record's concrete declaration, validates and copies its data, then checks its BAML implementation. It is not registering Python behavior, and it must not change a dependency's Record projection to Live. If a method later returns an interface view, that view retains the BAML copy; editing the Python record does not mutate it.

This differs from accepting an arbitrary native list as Iterable. A generated record provides a declared concrete source type; a native collection plus an existential destination does not provide that evidence. Preserve the existing explicit typed-data factory/host-adapter choice for collections. Preserve full field validation, declaration identity, generic arguments and associated pins on the record path too.

The current early inbound annotation check lacks the implementation-rule context for this record case. The repair belongs in shared checked admission, not in a Python-only cast or annotation exception. Plan F40 records the failing probe, passing limited checks and required tests. Its initial direct-interface/union input projection also leaves nested direction, required interfaces, conditional implementations and host sources open. This review changed internal documents only; the BEP and website remain on hold.


### F41. One authoritative completion check, including copied implementations

F40's record failure is repaired without changing its ownership model. The runtime resolves the copied nominal source and its generic arguments, checks the requested interface using VM implementation facts, validates the fields and materializes the BAML copy. Exact associated pins remain part of admission. The generated Python case now passes; the shared test demonstrates that later native edits do not change a retained view of the copy.

The host-return probe exposed a second rejection in both native and WASM transport design: a name-only return validator ran before the engine could consult implementation rules. Remove that competing decision. The engine already retains and traces the exact return/throws contracts for every host-call sysop, validates results on reacquiring its permit and reports HostContractViolation before resuming BAML. This also keeps runtime-created declarations and copied records on the same authority path. Transport envelope/receipt checks and strict engine field, scalar, JSON and throws checks remain necessary.

The new callback test accepts a Record<float> implementing the requested interface, but rejects both a Record<int> with wrong associated pins and an int field falsely annotated as float. An unrelated old test assumed UnknownError was copied; its behavior now makes it a retained concrete receiver under the existing projection policy. The updated test checks the actual receiver and reads its preserved data/message in BAML, while retaining the trace assertion. This changes the test's obsolete representation assumption, not error semantics.

See plan F41 for execution evidence and the remaining parity gaps. The BEP and website are unchanged.

### F42. Required membership and inherited calls need separate evidence

The transitive Python probe exposed an unresolved intermediate associated projection in compiler-exported requirements. Normalizing those requirements under the root's compiler facts lets every backend consume the same relationships. Keep root parameters symbolic and preserve an unspecified associated binding as unspecified; a requirement bound does not acquire an exact existential pin merely because the required declaration has a default.

Python now has limited positive/negative evidence for required-input acceptance on both Ref and Input roles. That does not finish the intended user API: an IteratorRef must also offer the compiler-resolved `iter()` caller directly. Current source still omits that inherited caller and binds bridge methods through the named declaration's own method list. Record this as an open implementation gate, not a reason to introduce a user conversion method.

Generated signatures and checked dispatch must agree on the declaring view chosen by BAML's member rules, including shadowing, diamond deduplication, ambiguity and concrete-Self restrictions. Required views use the same receiver and owner; their leases are separate references to it, not copies or extra host registrations. Tests must cover input acceptance and direct inherited invocation separately, including associated Error, shared state and release while another view or admitted call remains alive.

This continuation only updates internal design/review/plan documents. Plan F42 records the existing checks and follow-up work. The BEP and website remain unchanged.

### F43. Compile caller selection instead of reconstructing it in the bridge

Keep an interface's own required/default declarations for implementing it, and a separate compiler-resolved caller surface for using it. The latter includes inherited signatures and their declaring obligations, or ambiguity information. Resolve under private root variables before rebasing method-owned generic slots; an inherited method can otherwise accidentally capture a same-named root parameter at the same index. Associated bindings and callable effects remain part of the exported contract.

Runtime bytecode now carries the compiler-selected declaring-interface template for each callable name. It is traced/relocated with the other type metadata. Binding validates the root view, realizes the declaring obligation against the existing receiver, checks that obligation and binds the selected implementation/default with its own frame. A bare Self result retains the caller's original root view. Root shadowing, diamonds and ambiguity consequently use compiler resolution in both generation and emission. The artifact format is bumped to 5 for the changed serialized layout.

Python generates inherited methods from that caller surface. Its runtime wrappers prepare a call before returning an awaitable, so closing the originating SDK wrapper cannot revoke an already admitted call. Interface arguments and method type tokens use the receiver's typemap, as concrete calls do. This changes no field-mutation or host-registration policy.

The full unpinned requirement probe exposes a remaining native projection gap. Knowing that a receiver satisfies `Base<string>` does not tell the native caller its associated Output/Error. The compiler and engine retain that relationship, but Python's normal caller generator currently fails on the inherited projection. A passing pinned example must not conceal that package-generation failure. Complete the checked specialization/dynamic caller API and error reporting before claiming all-interface support. See plan F43 for passing checks, the exact failing probe and incomplete language parity. BEP and website remain unchanged.

### Feedback continuation: explain fixed types before generic handlers

This pass changes internal documentation only. The codegen review now distinguishes associated bindings fixed for a view, method type parameters selected per invocation, and required-interface bindings that the current view does not expose. A fixed `Source<Output=string, Error=ReadError>` gets an ordinary string-returning host method; associated types alone are not a reason to require `GenericCall`. The checked specialization described in plan F44 is an advanced path for missing evidence, not a mandatory conversion for concrete clients or fully specified inherited calls. F44 remains in progress; this pass adds no execution evidence.

The next external revision should use complete, task-oriented examples: use a generated client, run an Agent, call a returned interface, implement ordinary host behavior, then introduce the advanced handler. Each example must show its generated signature, construction, invocation, success type and error channel. Non-class examples use the same checked references and implementation rules; the array-versus-native-list distinction is an ownership/conversion decision, not a restriction to class receivers.

The plan now tracks these example packets separately for generation, native type checking and actual bridge execution. Python/TS lead the explanation, with Go/Rust comparisons; that ordering does not defer the other bridges or narrow the original scope. Existing implementation work is preserved. Neither the BEP, website nor implementation was edited in this feedback pass.

### F44. Explicit projection obtains missing associated evidence

Python now supports `await value.as_interface(TargetRef[...])` for a fully specified generated interface ref belonging to that receiver's SDK. It runs typed `baml.identity` through existing owned call preparation: check membership, preserve the receiver and retain the exact selected view. This neither copies the implementation nor registers another host object. Ordinary concrete client inputs and fully specified inherited calls remain automatic.

The generator's fallback is deliberately specific: an inherited method needs unspecified associated bindings belonging to its own declaring interface. Keep its compiler contract, document the explicit projection in the original Ref docstring and generated `INTERFACE_PROJECTIONS.md`, and expose the ordinary callable on the fully specified declaring Ref. Do not pretend this solves arbitrary dependent types. The full unpinned Python fixture now generates and executes; exact result typing and ownership checks pass. See plan F44 for test counts and remaining language parity.

### F45. Background error delivery is a typed value boundary

The native probe found two independent losses. Error delivery attached no issuing runtime to adopted refs, and engine reporting used trace conversion, which copied an implementing object instead of preserving its declared interface view. Fix both; changing only native handle ownership cannot recover an interface type already erased by the engine.

GC now keeps the failed future alive until export, using its existing tracing of the error value and exact throws-type heads. The engine roots that future while awaiting a permit, then applies ordinary typed boundary conversion. Reported future state prevents duplicate reports. Panic values bypass the authored throws contract as on foreground calls; conversion failures become structured SDK panics. The reporting queue stays rooted across permit acquisition so cancellation during that await does not discard pending reports.

The native owned error carries its original runtime and admission session. The installed handler captures the runtime weakly to avoid an ownership cycle. Receipt adoption uses a separate delivery session: shutdown must not prevent error inspection, but it must prevent new method calls on a closed issuer. Python uses this authority for calls; Node retains it for its pending reference-method port. Host-callable argument delivery remains separate unfinished work. A private Python GC maintenance hook drives real collection in tests while the runtime stays open. Verification is recorded in plan F45; the BEP and website remain unchanged.

### F46 — Callback authority follows the issuing engine; codecs follow registration

An owned argument receipt alone is insufficient when an argument can itself call BAML. Attach the original engine binding and invocation session to the host-call delivery. The engine stores a weak issuer so installing a bridge does not create a self-retaining runtime. Receipt adoption and permission to start a later method call are separate: retaining an argument does not switch it to a replacement runtime.

A callback also belongs to the SDK context that registered it. Python dispatch must use that captured typemap for arguments, user code and result/throw encoding, including any nested callable registration. Reading the latest global typemap at dispatch time can decode an interface into the wrong generated class or fail after another SDK is imported. This checkpoint implements the Python context capture; other adapters still require corresponding review and evidence.

The F45 private Python GC hook was unnecessary. The existing `baml.sys.collect_garbage()` operation provides the required deterministic collection on the selected runtime, so ownership tests use it and the extra hook has been removed. This is an implementation correction, not a new public interface API.

F46 probing exposed a callback re-entry deadlock after rebuilding the addon: the callback's private asyncio loop blocks an async Tokio worker while a BAML method it awaits can remain queued on that worker. Python stack diagnostics showed both loops waiting; the native sample captures the dispatch stack. Schedule this blocking dispatch on Tokio's blocking pool. This keeps ordinary callback re-entry runnable; it does not establish application-loop affinity or bounded cancellation of arbitrary blocking host code. The shared counter callback is the end-to-end acceptance case. Replace the old heuristic/xfail callback-release test with real collection through the existing public builtin while the default runtime stays installed.

F46 also found an independent copied-value bug in callback outcomes: the wire's sparse type annotation names the class, while its field payload has no repeated class name. Strict validation must keep that nominal information when checking the actual fields. Restoring the name/type arguments is sufficient; coercing fields first would incorrectly accept an integer returned for a float field. This rule applies to callback success and declared-error values, alongside exact retained-reference validation.

### F47 — Directional container and callable types

Python input types describe what BAML may read from the host. Container parameters use `Sequence`/`Mapping`, recursively admitting concrete implementations at interface positions; results remain ordinary `list`/`dict` values with checked interface refs. The encoder accepts those read-only container APIs. Strings and byte buffers remain scalar wire values, despite Python's `Sequence` subtype relationship; supplying those where BAML requires an array still fails runtime checking. BAML's invariant class arguments keep canonical native type arguments instead of substituting an Input view inside the class.

Host callbacks receive output types and produce input types, either directly or through an awaitable. BAML callable values receive input types and return output types. Alias companions and optional callback Protocols preserve that distinction. Required callback arguments are positional; optional arguments use exact BAML keywords, represented by `Unpack` of a closed `typing_extensions.TypedDict`. The native typing probe rejected ordinary optional-keyword handlers when that dictionary was open; `closed=True` expresses the actual finite keyword set and fixes their acceptance. Protocol type variables are separate inferred-variance parameters specialized with the enclosing type choices. Generic optional callbacks whose keyword cannot be expressed as a Python identifier currently fail generation with a diagnostic; they still need a checked keyword-adapter route before claiming complete coverage. This is an outstanding limitation, not a silent `Any` fallback.

Returned Python BAML closures now keep their original runtime and typemap, and pass back as the same native callable carrier. Copying creates an independent lease; `close()` releases only that lease. Existing synchronous invocation remains. Awaitable host callbacks use the callback's existing dispatch loop; routing to an application-owned loop remains separate work.

The final F47 generated consumers pass positive typing and reject fourteen invalid calls. The shared Python fixture passes twenty runtime cases, including optional callbacks on a generic interface method; native reference/callback tests pass forty-one cases. These are Python results, not cross-language completion. The unchanged parity ratchet still fails, with the eight new canonical cases missing in the other nine environments. See plan F47 for commands, logs and remaining implementation gates. The BEP and website are unchanged.

### F48 — Node invocation authority and local lease release

A Node SDK runtime object now binds one engine weakly and retains its original admission session. It cannot switch to a subsequently installed engine. An adopted native reference retains its own issuer strongly while usable. New calls check that issuer's session, and argument decoding consumes transferred leases even when admission rejects the call.

Native interface and callable calls prepare their receiver and arguments before returning a Promise. Closing the originating handle releases only that local lease; it does not revoke a copy or an admitted operation. Closing a provisional wrapper also leaves receipt cleanup intact: the receipt keeps its own owner until adoption or discard. Interface method type arguments use declaration-slot order, with empty wire `type_var` fields, rather than the free-function named-binding convention.

Encoding must roll back both host registrations and cloned native handles when a later value fails. This applies to function arguments, callback results and callback throws. When a decoded host error resolves to its original JS exception object, the decoder releases the now-unused transport-handle lease during receipt adoption. The original exception remains the returned object; cleanup need not wait for JS garbage collection.

This implements and probes Node's native operations beneath the intended generated API. It does not yet supply generated TypeScript interface classes, associated-type input evidence, SDK-captured codecs or callable wrappers using these owner operations. Browser/worker transports still need their own implementation. Keep those as explicit remaining work rather than describing bare native-handle tests as user-facing interface support. No BEP or website changes.

### F49 — Generated SDK context survives imports and asynchronous callbacks

Each generated TypeScript SDK now has an internal `_sdk.ts` module holding its runtime binding and typemap. Root and nested modules obtain function factories from that module. Its factory exports are hoisted functions; their closures read the SDK context only when a user invokes a call. That permits the existing lazy typemap's cyclic namespace imports without selecting a global runtime or initializing a second engine. Handwritten bridge factories capture the current runtime/map when constructed.

Codecs receive the map explicitly. This includes nested class type tokens, callback argument decoding, asynchronous completion encoding and declared-error decoding. A callback keeps the map selected when it was registered; a later global map change cannot alter its result type. This does not depend on keeping a mutable global context active across a Promise.

Returned Node closures now call their original owner and decode with their captured map. A private WeakMap identifies them on inbound encoding, so passing them back forwards the native callable handle instead of registering a JS trampoline. The existing synchronous caller shape remains; a public async/lifecycle callable API and hidden-host-callback sync restrictions are still unfinished.

The full generated Node SDK imports and passes strict typing for a consumer using copied records, root functions, a nested generic builtin, callbacks and returned closures. It executes after changing the default map and rejects both root/nested calls after replacing its engine. Interface declarations themselves still translate to `unknown` in TypeScript; this evidence must not be used to claim generated interface support. Browser/worker native ownership, direct runtime helpers, streams/introspection and other remaining context paths need their own verification. The BEP and website are unchanged.

### F50 — TypeScript callers, input evidence and concrete objects

Generated TypeScript interfaces now have separate caller and input roles. A `DecoderRef<string, never>` exposes `decode(image): Promise<string>`; its corresponding input role retains both Output and Error as invariant choices. An internal symbol-keyed witness represents a checked view. A concrete class with several implementations carries several overloads for that witness, rather than combining them into a wider implementation. Neither these declarations nor a fabricated JavaScript shape authorize runtime dispatch; the native receiver remains authoritative.

Input translation recurses through read-only arrays/maps, unions, alias companions and callbacks. A host callback consumes decoded output values and returns an input value or `PromiseLike` completion. A returned BAML callable reverses those directions. Class arguments remain canonical rather than replacing an invariant class instantiation with input-role parameters. Full typed generic-token evidence remains a separate gate: the current `BamlTypeToken` is not parameterized by its native result type.

The compiler's class projection also controls Node decoding. Live BAML classes become retained generated classes with factories and async methods. Their signatures and dispatch patterns come from the compiler-selected concrete implementation, including narrower concrete results, generic methods and concrete Self. Record classes keep their copied representation and can carry live children. `ResponsesClient.new_async(...)` now returns a `ResponsesClient`; it calls BAML methods and is accepted directly by `Agent.new_async({ client })`. This has an offline generated probe; it does not establish `Agent.run` or an LLM override's full execution/streaming path.

Interface and concrete wrappers share the existing owned native-call machinery, codec capture and independent lease cloning. They are frozen after construction, expose no writable BAML state fields, and reject ordinary JSON serialization. Methods may still mutate the BAML-owned receiver. Copied records remain ordinary editable native data. JSON/database export needs an explicit application-data operation; TypeScript's concrete data-export helper is still pending.

Runtime-owned classes such as `Stream` and `FunctionSpec` preserve their actual runtime constructor identity. Generated local type declarations add their compiler-proven input evidence; ordinary named imports cannot be declaration-merged. Static runtime factories that return the underlying runtime type still need a complete specialized input surface. Do not claim every runtime-owned construction path is covered merely because BAML-returned values carry the right generated type.

New generated helper names and private type variables are allocated against authored declarations and namespace bindings. Lifecycle/codec method collisions receive a suffix. A BAML method named `then` must also receive a suffix: exposing it literally would make the ref a JavaScript thenable, and Promise resolution could invoke it unexpectedly. Probes cover `then`, `close` alongside an authored `close_`, helper-name collisions and generic variables colliding with authored type names. Broader generator spelling/context-option collisions remain on the implementation checklist.

Unspecified inherited associated pins still prevent an unqualified caller. Such members have an explanatory generated comment, with no `unknown` substitute. TypeScript's checked specialization API is not yet implemented; it must land before the port is considered complete. Host implementations, scopes/cycles, full callable lifecycle APIs, browser/worker transport support, other SDK languages, complete client/Agent execution, media and canonical aliases also remain open.

F50's standalone process probe found a lifetime problem that the method assertions missed: the program printed success but never exited. A callback registration retained a referenced Node delivery channel until BAML GC released the registration. Explicit GC let the same program exit, which isolated the cause. Success output alone was insufficient evidence.

Keep receiver ownership separate from process activity. Callback channels now retain their callable without pinning Node's event loop. The public SDK exit hook waits for BAML work before closing admission, allowing background callbacks to re-enter BAML. The engine's observational `wait_until_idle` does not revoke references or prevent later calls. A weak Node runtime registry includes retiring runtimes; it does not keep idle heaps alive. Ordinary shutdown follows that wait and performs final collection/error delivery. Explicit shutdown retains its existing admission-closing behavior.

Six bounded subprocess tests pass without a forced exit, manual GC or shutdown: completed callbacks, intentionally retained callable values, delayed foreground work, background re-entry, callbacks from a replaced runtime and background errors. Three actual generated consumers now exit with status zero. This verifies automatic Node process completion; it does not prove bounded memory in a long-running singleton, cross-runtime cycle collection, worker isolation or arbitrary `beforeExit` revival. Keep those acceptance gates separate.

### F51 — A checked TypeScript projection needs typed, persistent evidence

Generated `Ref.type(...)` constructs a complete interface type, with ordinary arguments followed by associated bindings in declaration order. `await value.as_interface(target)` uses the existing owned identity call to validate that view and retain the same receiver. It does not bind a host object or change the implementation's associated types. Concrete client arguments still need no `.as_client()`.

The type selector accepts invariant `BamlType<T>` values. `BamlType.from("string")` and `reflect.Type.of("string")` preserve string statically; Never, ordinary generated record constructors and typed array/optional composition have corresponding paths. An opaque reflected schema remains `BamlType<unknown>`. An erased, branded `BamlTypeValue` is used where the wire accepts any type definition; it is not proof of a particular T. General generic class/enum type constructors and method-type inference still need implementation.

The target captures its generated Ref constructor and checks it against the receiver's original SDK. Composed tokens carry their nested checks; ordinary record constructor tokens also require exact generated constructor identity. A subclass with the same methods or inherited factory is not that generated codec. Checks on type arguments occur inside transactional argument encoding, so failure after encoding a receiver releases the provisional wire leases. Wrong runtime Output/Error choices reject without invalidating existing references. Python exposes this invalid-input path as TypeError, separate from a declared BAML application error.

Probing found a declaration-emission trap: TypeScript drops private field annotations from `.d.ts` files. A private `(T) => T` field therefore did not preserve token invariance for consumers of the runtime package. Use an internal symbol-keyed field whose full signature survives emission. Apply the same rule to concrete refs: their base type carries the declaration name and generic arguments as invariant pins. Checking generated source alone is insufficient; compile a consumer against emitted SDK declarations too.

A separate wire limitation surfaced while composing reflected types: `BamlTyDef.witnesses` describes the root class, not arbitrary classes in its definition tables. Moving that root into an interface argument, array or optional would change the witness owner. These compositions now reject root-scoped witnesses explicitly. A later shared graph change must carry their owners before this case can work across all bridges; do not silently drop them or invent a host-only representation. Ordinary class/enum definition tables are copied and deduplicated, with conflicting same-name definitions rejected.

Verification and remaining gates are recorded in plan F51. This changes internal design/code and probes only; the BEP and website remain on hold.

### F52 — Method generics need typed evidence; enum values need contextual decoding

A TypeScript caller previously supplied an unparameterized wire token while TypeScript inferred `T` independently from the value. This could claim a native result type that disagreed with the selected BAML type. Generated interface and live concrete methods now require `BamlType<T>` for each method slot and suppress inference from arguments. A string token produces `string`, even for the literal input `"Ada"`. The invariant token relationship must survive emitted `.d.ts` files, including callback parameter/result positions.

Nominal declarations get companion factories: `BoxType(BamlType.from("string"))` produces `BamlType<Box<string>>`, without creating a Box. Enum factories take no arguments. Factories use the generating SDK's typemap rather than a mutable global default, and validate nested evidence against that SDK. Names are allocated against authored declarations and namespace/helper bindings. Interface selectors remain `SourceRef.type(...)`. Builtin carrier declarations are excluded using `compiler_aliases`; they are not independent nominal types. Runtime-owned nominal classes use the same companion mechanism, without modifying shared runtime constructors.

The shared type-graph composer preserves child class/enum definitions and rejects conflicting same-name definitions. It still rejects root-scoped witnesses when embedding their owning type under a different root; the protocol must gain explicit witness ownership before supporting those compositions. No child definitions or conformance evidence are silently dropped.

A real generic enum roundtrip exposed an existing representation gap: TypeScript string enums encode as strings. An exact enum contract can supply that missing identity, just as an exact class contract supplies identity to an unannotated record. Inbound conversion now reconstructs an enum only in that context, then validates the member against the retained declaration. Explicit source annotations cannot be relabeled. Read-only union selection consults enum membership so nullable enum inputs work; overlapping enum/string alternatives continue through the existing ambiguity policy. Strict callers must identify one selected alternative. This does not make a native string implement a BAML interface.

The full generic API remains unfinished. Free-function inference and optional `$types`, generic copied-record constructor metadata, directional generic interface inputs, static bound projection, ordinary/advanced host implementation registration and other backend ports still need work. In particular, TypeScript's `number` does not distinguish BAML int from float. Runtime validation remains required. See plan F52 for execution evidence and the final limits; the BEP and website remain unchanged.

### F53 — Host registration must retain method quantification

A required interface method has no implementation function from which to recover metadata. Its value signature also cannot describe all its generics: `function check<T extends Marker, U>(self) -> string` has the same argument/result shape for every choice of T and U, but T still has a bound and both parameters still require type arguments. Inferring registration requirements from that shape would accept an incomplete contract.

`InterfaceMethodDef` now preserves the complete dense runtime generic frame and its bounds, using the same compiler lowering as ordinary functions. The frame is `[Self, interface arguments..., method arguments...]`; associated bindings remain projections, and compiler-only effect variables are excluded. Empty bound sets and unused parameters keep their slots. The shared head visitor covers declarations referenced by these bounds, including their nested arguments and associated pins, during linking, tracing and moving-GC fixup.

Checked host-entry interface invocation now validates this declaration's arity and bounds before resolving the implementation function. Validation of the selected implementation remains as a second check. An engine test deliberately removes a provided function's bound and confirms that the interface declaration still rejects an incompatible choice. This is necessary groundwork for a host adapter whose implementation is a bridge trampoline.

The added serialized fields require artifact format 6. SDK bytecode must be regenerated and native runtimes rebuilt together; old-format artifacts fail the existing initialization check. This is not a C ABI registration endpoint or a completed host implementation API.

Registration still needs immutable adapter-type creation, callback trampolines that carry realized generic frames, exact associated/required-interface validation and coherent atomic publication, followed by the native SDK binding/encoding paths. Existing structural reflection witnesses are insufficient: they reject bodyless required methods, validate required interfaces by head alone, and have a documented overlap gap against blanket implementations. Do not reuse them as an unchecked host-registration shortcut. The BEP and website remain unchanged.


### F54 — Validate registration in a private proposed world

Checking each implementation against only the current table is insufficient. Suppose a static blanket says `implements<T extends Gate> Pick for T`. Registering both `Gate` and an explicit `Pick` for a fresh adapter conflicts, even though neither existed before the batch. Reversing registration order must not change the result. Likewise, two implementations of `Choice<int>` conflict even if they choose different associated `Item` outputs; associated bindings do not participate in selecting an implementation.

The runtime resolver now supports a private slice of staged immutable rules. Recursive bound checks see both the existing rules and the complete staged batch. Before publication, every proposed concrete receiver/interface-input pair must select exactly one applicable rule. The check uses ordinary semantic type equivalence, not spelling or raw pointer equality of type expressions. Existing rule pointers reached through multiple package paths are counted once. This gate is for fully realized dynamic registrations; source-level generic implementation coherence remains the compiler's responsibility.

Direct reflected class creation and recursive builders now prepare and validate their rules before exposing the new type. A rejected builder registration leaves all connected builders unresolved and retryable. Accepted rules enter the weak dispatch index in one write-lock operation; the table does not retain their receivers or turn into a global ownership root. Correction from F55: class-to-rule ownership was assumed here but missing; the older GC tests rooted both independently. F55 adds the actual ownership edge and receiver-only tests. No provisional rules are temporarily published, so rollback does not need to remove globally visible registrations.

This repairs the overlap gap identified in F53 and supplies a shared mechanism for future host adapter registration. It does not finish that API: full required-interface instantiation checks, host-field rejection, method contracts, ordinary-function trampolines, generic host frames and SDK binding still need implementation. The current reflection requirement check by head alone remains a known gap; the private overlay must eventually serve the complete conformance validator as well. See plan F54 for verification. The BEP and website remain unchanged.


### F55 — A weak dispatch index needs a real heap owner

Host method dispatch now uses ordinary VM functions. Each instance retains its own callbacks; a method realizes the declaration signature in the current generic frame before creating the host closure for that invocation. This preserves ordinary generic calls, associated output substitutions, BAML defaults and bound method values without a separate dispatch opcode. It does not yet deliver unused generic choices to advanced GenericCall handlers or provide a host registration API.

The first runtime probe failed because compact bytecode folded a runtime helper's placeholder Null constant into LoadNull, discarding its resolved callable pointer. Compaction now preserves resolved heap constants. The helper and wrapper also need a runtime package owner: only runtime-owned functions trace and forward their resolved constant pool. These functions and pointers are runtime-only, not portable artifact payloads.

A stronger GC test found a separate pre-existing ownership gap in reflected witnesses. The weak table intentionally does not trace rules, but a fresh class had no owner connecting it to those rules. The old table tests rooted both objects explicitly, masking this gap. Fresh witnessed classes now retain a private package that owns their rules; host adapter methods use the same package graph. The resolver deduplicates rules reached through both package and index. The private package is not registered globally, so unreachable package/class/function cycles can be collected. This corrects the earlier F54 ownership claim.

The new VM tests root only a receiver or class, collect before invocation and during callback suspension, and verify release after the last owner disappears. An engine test exports a retained interface from a manually assembled adapter, collects, and calls it through real native callback dispatch. It verifies generic calls, a BAML default, a bound method, an associated output and invalid return/throw rejection. After each case, collection releases the callback while the engine remains alive. The final engine/VM selection passes 13/13. The test dispatcher decodes the outbound wire call and completes with in-process external values; it does not exercise a language SDK or the C completion decoder. These tests do not prove cross-language cycle collection, automatic collection scheduling, singleton memory bounds or native SDK binding.

Source probing also found that interface declarations currently reject both parameter defaults and TypeScript-style optional syntax. The optional-slot VM test builds existing runtime metadata directly; source syntax and generated SDK behavior remain an explicit gap. The BEP and website remain unchanged.


### F56 — Validate the declaration's constraints, not just its interface names

A structural witness previously satisfied `requires Parent<T, Item=Self.Output>` if the batch contained any Parent witness. That loses both T and Item. Generic-argument bounds and associated-type bounds also need checking when dynamic types are registered; source annotations do not validate arbitrary runtime requests.

Compiled interface metadata now carries predicates over `[Self, interface arguments...]`. Each predicate pairs a subject with an interface constraint. A generic bound uses its parameter slot, a requirement uses Self, and an associated bound uses the compiler-qualified Self projection. Constraints preserve only their written associated pins; an omitted pin does not silently become a default. Reflection metadata and predicates are emitted from the same lowering operation. The head walker includes both predicate halves for linking, tracing and moving-GC fixup. The added serialized field advances artifact format to 7. Native runtimes and generated bytecode must be rebuilt together.

The registration gate first requires each declared associated binding exactly once and preflights every staged template/coherence goal. It then realizes and proves each declaration predicate against that same proposed world. A required interface may come from an applicable blanket implementation; the caller need not add a redundant explicit witness. Failed checks leave dynamic dispatch unchanged and builders retryable. Required-interface failures retain their diagnostic category instead of being mislabeled as overlapping implementations.

An additional context bug surfaced during this work: substituting a projection through BexVm constructed a new published-world resolver, losing staged rules and the selected package. ImplResolver now supplies its own substitution context so projection and membership keep the same world. Structural equivalence still uses its intentionally opaque nominal context to avoid recursive equivalence/membership queries.

This checks interface declaration predicates. It does not finish host adapter registration: provided-method parameter/result/effect variance, generic quantification, field rejection, receiverless methods and SDK binding still require their own gates. The final VM/compiler selection passes 53/53, targeted GC checks 2/2, artifact checks 5/5 and public BAML registration cases 7/7. Plan F56 records snapshot checks and the initial nextest leaky-process observation, which did not recur in either rerun. The BEP and website remain unchanged.


### F57 — Ascribe one BAML contract and retain every host receiver

The previous plan asked the runtime to validate a second native method signature against the BAML interface, including universal generic variance. Ordinary host callables already use a simpler boundary: the BAML parameter supplies the contract, and actual host calls/completions are checked against it. Apply the same model to interfaces. A host adapter descriptor selects an interface and its provided methods; the runtime takes each method's complete signature, modes and generic frame from the retained declaration. The descriptor cannot drop an unused generic, narrow a parameter, widen an output or change the throwing effect.

Native compatibility checks still matter. Generated Host contracts and native type checkers enforce compatible overrides where supported; binders verify callable presence and calling convention where inspectable. A broader native input or narrower native result is usable under the interface contract. Untyped annotations are not runtime proof, and binding must not trial-run host code. Every crossing retains the actual-value contract checks. Full native positive/negative codegen tests and binder implementation remain required; F57 does not claim they are done. This is an architectural change to the earlier proposed redundant-signature ABI, not a relaxation of the BAML method contract.

The new VM API separates type registration from instance creation. Descriptors use a `[Self]` template for interface inputs/associated pins, so recursive receiver choices can be resolved before publication. Registration checks required/provided/default methods, inherited field rejection and declaration metadata, then uses F54/F56's shared coherence and constraint gates. It publishes the weak rows once and returns the callback order. Distinct registrations have distinct nominal identities even with equal names. Runtime-local result types must be rooted/exported before releasing the heap permit; this is not an owning SDK handle or a C ABI endpoint.

An instance must retain its original host object even if it provides no methods. Marker interfaces and implementations using only BAML defaults cannot rely on a bound callback to keep that object alive. Every instance now has an opaque receiver slot followed by its callable slots. The type owner keeps a callback count only. Instance construction validates the registered class, receiver kind and complete callable vector before retaining anything. Retaining a type therefore does not retain its dead instances.

The existing VM and engine tests now call the checked registration/instance operations instead of hand-assembling rules and private packages. Added tests cover rejection without publication, Self pins, nominal identity, callback admission, default override and marker/all-default ownership. The final F57 log records 12/12 passing VM/engine tests; the plan distinguishes that result from remaining final review and native SDK checks. Receiverless host implementations, advanced frame transport, SDK binders and native signature tests remain pending. Artifact format stays 7: the added package marker is runtime-only and is not serialized. The BEP and website remain unchanged.


### A13 — Review follow-up: one contract, separate checks

The current plan still described a second runtime native-signature validator after F57 had removed that proposed ABI. Reconcile it with design §7.3: BAML supplies the method contract; generated native types and binders check what their language can establish; call admission and completion check actual values. A successful bind is not proof that arbitrary host code will always behave correctly. The codegen review now explains this through one Source implementation in four stages, preserving exact associated Error even where native errors are untyped.

The same review explicitly separates an existing BAML array implementing an interface from a native collection requiring a checked conversion. This avoids suggesting either class-only interface support or automatic registration by native shape. The existing Python/TS/Go/Rust signature tables and evidence packets remain the publication gates.

This pass updates only internal documentation and records an already completed F57 test log. It does not change the BEP, website, implementation, language scope or field/mutability boundary.


### F58 — Registration results must own their type graph

The VM registration result contains movable pointers and cannot safely cross an await or released heap permit. The engine now accepts a descriptor whose nominal heads are either program names or retained declaration handles. Slot zero remains the new concrete Self. It resolves all heads under one heap permit, checks nested declaration kinds/arity, invokes the shared VM validator, then roots the resulting class and each callback's exact interface contract before returning.

`HostAdapterType` has private immutable storage and can be cloned. It retains types, never receivers. Instance creation verifies the issuing runtime and delegates complete receiver/callback admission to the VM. Its returned instance handle owns the host object even for marker interfaces. A Type reflection wrapper is not accepted as a declaration head. This distinction is internal engine API; the future bridge endpoint must translate its checked type evidence, not expose raw declaration pointers.

Interface projection now also accepts retained type evidence. This lets a registered callback contract select its exact interface view, including an associated runtime-created type, without reconstructing declaration identity from names. Existing name-based projection delegates to the same operation. The new engine tests exercise this path after GC; the dispatcher test now uses engine registration and instance creation instead of holding a VM permit throughout setup.

Final checks: 16/16 VM/engine tests and 2/2 existing interface-view tests pass. The plan records initial compile fixes and an intermediate nextest subprocess-leak observation that did not recur in the final run. These forced-GC tests do not prove automatic collection scheduling, cross-language cycle collection or SDK registration. Shared bridge ABI, native binders, receiverless providers and advanced frames remain open. No design scope was reduced and no BEP/site edit was made.


### F59 — Registration is a protocol capability, not a BAML value

The shared bridge now has explicit request messages for registering an adapter type, creating an instance, and projecting an interface view. A registration template uses frame slot zero for Self and later slots for positional type evidence shared across all its implementations. Named types, explicit portable definitions and retained type references use the existing type-evidence decoder. Binding an entire interface through a frame slot preserves a runtime-created declaration too. Missing slots reject; they never become unknown.

Adapter type registration gets its own handle-table tag. It owns immutable type metadata and follows ordinary clone/release and receipt adoption, but cannot be passed into BAML as a value. The table-to-value conversion is therefore fallible. Class/callback type roots in the registration response remain internal reflected-type evidence: the engine checks the actual Object::Type and runtime provenance when they are used, even if a protocol caller supplies another kind of heap handle.

Preparation pins borrowed registration/type/receiver keys before async execution. Instance inputs are captured as a complete transferred batch before validating the registration target. A final release guard drains abandoned host registrations after their owners drop, including preparation failures that never enter the engine. Outbound registration metadata is one receipt-owned aggregate; abandoning or partially adopting it must release every unclaimed key.

This checkpoint implements target-neutral Rust bridge helpers and protobuf messages. Exported C/Wasm entrypoints, SDK implementation wrappers/binders and their native execution remain pending. The four shared bridge integration tests pass, and the protocol clients have regenerated. The final broader type/engine/ownership selection passes 66/66; normal C++ generated-source checks pass 2/2 (plan F59). No public SDK support claim or BEP/site edit follows from this checkpoint.


### F60 — Administrative outcomes share ordinary delivery ownership

A private `HostOperationRequest` selects registration, instance creation or projection. `HostOperationResult` distinguishes registered metadata, an ordinary value, or a failure using the existing BAML error/panic envelope. This avoids pretending a registration descriptor is an application value and avoids a second error classification policy.

Python/Node private native methods use their existing `BamlEncodedResult` receipt. Preparation captures inputs before checking the selected runtime's admission state. The scheduled operation keeps that runtime alive; disposal or failed scheduling drops its input owners. A delivered result owns provisional keys until its decoder adopts or discards the complete aggregate. Native wrappers retain the original runtime authority; a later singleton replacement cannot redirect these operations. A closed receipt session rejects delivery and releases provisional keys.

These operations do not call user bodies and do not need an application call ID. Dropping a cancellable future releases its prepared inputs and unadopted output; this does not promise that cancelling a JavaScript Promise aborts native work. Checked interface method calls retain the ordinary invocation/cancellation protocol. Automatic callback/receiver retention, optional scopes and the known cross-language cycle limits remain as designed. C and Wasm delivery ports, generated binders and advanced callback frames are still required. Final execution evidence: 6/6 Python cases, 5/5 Node cases and 5/5 shared administrative cases pass, with two existing Python replacement checks also passing. The probes exercise the same retained host receiver, owner-language state changes, default methods and per-call rejection of an invalid host return. The Node protocol test also type-checks. Plan F60 records generation, build and failure details. These private native operations do not claim generated Host/binder or all-language support. BEP/site unchanged.


### F61 — Return every realized obligation, and index host operations

A callback slot's method name is insufficient when one object implements two interfaces that both declare that name. The registration response now includes every checked interface type in the submitted implementation order; each callback identifies an implementation index and authored member. This also covers all-default and marker interfaces, which have no callback slots. The engine owns this interface list once, and the response transfers independent SDK leases under the existing aggregate receipt.

Binders project new instances using those retained types. They must not reconstruct an associated portable definition and import it a second time: that creates a different runtime type even if its name and shape match. The new Python probe binds an all-default interface with a freshly defined associated type and invokes its default method using the original checked view. Another probe binds same-named methods to separate native bodies through their implementation indices.

The explicit Python binder inspects native calling conventions when available, rejects properties without evaluating them, captures the generated SDK's codecs, and reuses the ordinary callback dispatcher. Omitted default methods execute BAML defaults; provided defaults supply host bodies. No global binder cache retains a host receiver. Automatic implementation sources still need active-identity reuse without a receiver/reference cycle, so this explicit binding work does not substitute for the final automatic API. Generated typed associated/generic host descriptors and context injection remain gates. Public typed emission is now verified for ordinary nongeneric Python interfaces and fully ordinary required-interface graphs: generated `FooHost` protocols describe required native methods, and `await FooRef.bind(implementation)` returns the checked reference. Synchronous and asynchronous bodies type-check; absent defaults use BAML bodies. Unsupported associated/generic or ambiguous graphs do not get an `Any` binder. The generated implementation base/factory, automatic binding, optional scopes and host context injection remain unimplemented.

The fresh generated SDK passes four selected canonical execution cases; positive typing has no diagnostics and the negative probe rejects all four expected invalid uses. Native Python probes pass 9/9. Plan F61 records the other bridge/protocol checks and the still-failing SDK parity report. This is a user-requested stopping point, not completion of the full design. BEP/site unchanged.
