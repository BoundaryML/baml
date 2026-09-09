# Interface design probes

These probes informed [the technical design](/Users/aaron/projects/baml/baml_language/MEDIA_INTERFACES_AND_BRIDGES_DESIGN.md) and [the user guide](/Users/aaron/projects/baml/baml_language/INTERFACES_USER_GUIDE.md). They cover risky language/API assumptions and selected real runtime paths. They are not an implementation of general interface bridges or evidence that all proposed SDK examples work today.

Research checkout: `1c1342347`, inspected and probed on 2026-09-04. Compiler/interpreter versions are reported per experiment. No production implementation was changed for these probes.

Current PR boundary: passing data and invoking methods, primarily on BAML-defined classes implementing BAML interfaces. Owner-side stateful methods remain in scope; direct bridge field access, native field-bearing bindings and shared mutable-storage backends are deferred. Earlier field probes remain evidence about existing language semantics, not implementation requirements for this PR.

## Decisions supported or changed by the results

| Question | Evidence | Decision |
|---|---|---|
| Can generated concrete clients be passed without `.as_client()`? | Rust compilation/execution and strict TypeScript checks accept concrete clients, checked refs, and required-interface inputs. | Keep automatic projection at generated input boundaries. |
| Does ordinary `Agent.run(spec)` need an explicit output token? | Rust and TypeScript prototypes infer different exact outputs from two typed specs on the same agent. | Infer from the typed spec; keep explicit type selection for other cases. |
| Must a record containing a live object itself become a live facade? | BAML execution preserves shared journal/callback children across deliberately copied outer envelopes; ordinary SDK DTOs already copy their outer shape. | Use ordinary `RunResult.value` and `ModelTurnInput.prompt` fields. Preserve nested live refs; document outer-copy semantics. |
| Does an empty interface accept any matching shape? | The source-built BAML compiler/runtime accepts explicit marker implementations and rejects an unrelated identical shape. | Keep explicit nominal membership and host registration even for empty `media`. |
| Does listing all known implementations close an interface match? | Source-built BAML compilation rejects the match without `_` with E0062. | Require the wildcard for open interface matches. |
| Can Rust refs simply derive `Clone`? | The draft shape fails to clone a ref with a non-Clone output type. | Generate manual `Clone` implementations without output-type bounds. |
| Can Rust `ClientInput` inherit today's `BamlValue`? | The `Sized` supertrait prevents `dyn ClientInput`. | Separate the object-safe input capability from the bidirectional value codec. |
| Can options store a borrowed interface input? | A builder that outlives its borrowed concrete client fails compilation; an owning builder succeeds. | Clone local receiver ownership in the setter; validate the call when encoded. |
| Are TypeScript method declarations sufficient for exact pins? | Method parameter bivariance permits widening even in strict mode. | Use function-valued invariant brands and host callback properties. |
| Does a binder's token merely check an inferred output type? | The old signature jointly infers a wider Output from a wider token. | Use invariant tokens and `NoInfer`; deliberate widening uses an explicit generic argument. |
| Do TypeScript brands establish runtime membership? | `declare` brands emit no runtime symbol; casts bypass static checks. | Validate trusted reference state, session, implementation, and exact pins at runtime. |
| Can that TypeScript brand work across generated provider modules? | A separate consumer accepts a sibling provider facade through a public barrel; the raw host object fails. | Share one internal type-only brand declaration; omit it from public exports. |
| Is the concrete Python image callback path currently broken? | Matched current-runtime probes pass for both host-created and BAML-created images. | Preserve the observable behavior in the replacement design, without freezing its encoding. |
| Can returned BAML functions safely pass back through today's generic callback adaptation? | A real returned closure is re-registered as a Python callback and triggers a nested-runtime panic. | Recognize existing live capabilities before any host adaptation; pass back their original receiver. |
| Can Pydantic support a convenient native implementation API? | Pydantic 2.13.3 accepts a generated-style abstract mixin, enforces required methods, and preserves model field validation/dumping. A custom interface schema preserves an opted-in instance and rejects JSON interface construction/export. | Offer a generated Python implementation base with lazy automatic binding. Keep Pydantic validation independent of runtime binding. |
| Must every Python caller write a scope/destructor? | The ownership model releases a collected proxy while keeping its host alive for a separate BAML lease; explicit close releases once. A cross-runtime cycle remains until teardown. | Default to the runtime's registration scope and automatic proxy release; offer shorter scopes for deterministic teardown. |
| Can TypeScript/Rust also hide runtime binding in ordinary calls? | Both factory prototypes construct synchronously, accept direct generated-function inputs, preserve active identity, and forward default methods through the mock runtime. | Generate `Greeter.implement(...)` / `Greeter::implement(...)`; register lazily using the runtime's default scope. |
| Should Rust infer a complete BAML implementation set from `Arc<H>` and Host traits? | The direct Arc form compiles, but each input trait only supplies its own descriptor; the model rejects conflicting descriptors for an existing receiver. Rust cannot enumerate all Host implementations. | Choose an explicit owned implementation wrapper with a fixed interface bundle. Use explicit combined registration for unrelated interfaces. |
| Does Node 22 support `using` cleanup? | Node 22.22.2 exposes both disposal symbols but rejects raw syntax. TypeScript 5.8.3 targeting ES2022 compiles and runs synchronous/asynchronous disposal successfully there. | Generate disposal hooks; support Node 22 through downlevel compilation and recommend `using` for local refs, `await using` for bridge scopes. |
| Do interface fields represent copied state? | Actual BAML execution observes writes between the concrete object and interface view in both directions. A Pydantic adapter model reads/writes the same retained native object. | Keep state with its owner and expose authored methods in this PR. Direct live field projection is deferred. |
| Is a foreign root getter/setter enough for mutable aggregate fields? | BAML list/map/class aliases mutate the stored child without another parent-field store. The Python copied-list experiment loses those mutations. | Defer direct field/aggregate interoperability together. A future implementation would need shared identity; this PR uses ordinary data-result copies and authored methods. |

## BAML: membership, open matching, and mixed records

Files and full results: [BAML probe directory](/Users/aaron/projects/baml/baml_language/interface_probes/baml/README.md), [captured observations](/Users/aaron/projects/baml/baml_language/interface_probes/baml/RESULTS.md).

Run from `baml_language` with the CLI built from this checkout:

```sh
cargo run -p baml_cli --bin baml-cli -- --version
interface_probes/baml/run_probes.sh
```

The source-built CLI reports 0.18.0. A comparison run with the installed 0.18.1-nightly.20260901.a toolchain produced the same results. Three positive projects pass checking, execution, and their BAML tests; two negative projects fail with the expected errors.

Empty-interface membership is explicit: reflection and matching recognize the class with `implements Marker {}`, while a class with the same empty shape is not a member. Implicit assignment of the latter to the marker fails with E0001. An open-interface match listing both currently declared implementations still fails with E0062 when its wildcard is removed. Both observations agree with `TYPE_SYSTEM.md`.

The record experiment confirms that BAML class assignment, return, and nested field reads preserve aliases inside the engine. It then deliberately constructs a fresh outer envelope around the same journal. Appending through the copied envelope updates the shared journal to `[1, 2]`; replacing the copy's journal field leaves the original at `[1, 2]`, while the new journal becomes `[99, 100]`. Two fresh envelopes carrying the same closure observe shared captured state, returning `1` and then `2`.

These are real BAML semantics, not a demonstration of the proposed mixed-record bridge. Today's ordinary statically compiled classes already cross the SDK boundary as recursively exported structural values in [`conversion.rs`](/Users/aaron/projects/baml/baml_language/crates/bex_engine/src/conversion.rs:687); that path does not preserve all outer class identity. The design choice is to make the distinction explicit and preserve capabilities recursively, rather than making every parent containing one live object require async field access.

Consequently, `RunResult<Out>` becomes a native record with `value`, a live `journal`, and portable `usage`. `ModelTurnInput` becomes a native record with a directly callable `prompt` and live journal/toolbox/type children. Live objects such as Journal and Toolbox retain their own methods and shared state. Replacing an outer record field is local; passing the record back constructs a new outer value with checked live children. Entire records containing live children remain session-local and cannot be exported as portable JSON. Actual cross-bridge copied-envelope replacement and nested live-receiver identity checks are required when implementing that codec; this does not require direct field access on a live receiver.

## Rust: input ergonomics, generic refs, and ownership

Files: [runner](/Users/aaron/projects/baml/baml_language/interface_probes/rust/run.py), [positive prototype](/Users/aaron/projects/baml/baml_language/interface_probes/rust/positive.rs), and deliberately rejected alternatives in the same directory.

Run from `baml_language`:

```sh
python3 interface_probes/rust/run.py
```

Tested with `rustc 1.93.0 (254b59607 2026-01-19)`, edition 2021. This is not an MSRV check.

The positive executable verifies:

- Concrete `ResponsesClient`, `ClientRef`, and `StreamingClientRef` inputs use the same input trait and preserve one receiver identity.
- A heterogeneous `Vec<&dyn ClientInput>` can be constructed directly.
- An options builder owns a receiver after the original concrete wrapper leaves scope.
- Closing the model's scope rejects a later call while retaining memory safely; the receiver is dropped exactly once after the last owner is dropped.
- An interface ref with a non-Clone output type can be cloned with a manual implementation.
- One agent runs `FunctionSpec<Resume>` and `FunctionSpec<Invoice>`, inferring each output type.
- A lifetime-generic boxed-future method works through `Arc<dyn GreeterHost>` while borrowing the implementation.

All positive assertions passed. Five negative fixtures were rejected with the expected diagnostic:

| Fixture | Diagnostic | Meaning |
|---|---|---|
| `derive_clone_fails.rs` | E0599 | Derived Clone adds the unwanted output-type bound. |
| `sized_input_fails.rs` | E0038 | A Sized value-codec supertrait prevents a trait-object input. |
| `borrowed_options_fail.rs` | E0597 | An options object cannot outlive a client it only borrows. |
| `generic_dyn_fails.rs` | E0038 | A method with its own type parameter cannot be used as an ordinary Rust virtual method. |
| `container_coercion_fails.rs` | E0308 | An existing concrete-element Vec does not implicitly become an interface-element Vec. |

These are compiler tests and a local ownership model. The receiver uses `Arc` and an atomic revocation flag, not the BAML handle table. They do not prove native lease transfer, asynchronous cancellation races, or callback cleanup across the ABI. The existing source corroborates two implementation facts: [`BamlValue` currently requires Sized](/Users/aaron/projects/baml/baml_language/sdks/rust/bridge_rust/src/baml_value.rs:39), and [`FunctionSpec` already uses manual Clone](/Users/aaron/projects/baml/baml_language/sdks/rust/bridge_rust/src/spec.rs:145).

## TypeScript: input typing, exact pins, and generic inference

Files and exact commands: [TypeScript probe README](/Users/aaron/projects/baml/baml_language/interface_probes/typescript/README.md). Tested with TypeScript 5.8.3 under strict checking, including `exactOptionalPropertyTypes` and `noUncheckedIndexedAccess`.

The strict positive fixture passes and verifies expected static rejections using `@ts-expect-error`. The separate negative fixture exits 2 with six intended errors: unbound ClientHost, unbound DecoderHost, wrong ref pin, wrong model token, a wider union token, and function-property variance mismatch.

The binder's former shape:

```text
bind<O>(implementation: DecoderHost<O>, options: { output: BamlType<O> })
```

can infer `O = string | number` from a string-producing host implementation and a wider token. That can be a sound explicitly chosen output contract, but it is a surprising result if the token is intended to check the implementation's inferred output type. The revised signature uses `BamlType<NoInfer<O>>`; a deliberate union contract remains available through an explicit generic argument.

The variance probe distinguishes a method-shaped pin from a function-property pin:

```text
// Method bivariance can still admit widening.
pin(value: T): T;

// Under strictFunctionTypes, this checks both directions.
pin: (value: T) => T;
```

The generated ref and input brands use the second form. Host callback contracts also use function-valued properties where parameter variance matters. Applications can still implement them with ordinary contextually typed object methods.

The [cross-module fixture](/Users/aaron/projects/baml/baml_language/interface_probes/typescript/cross-module/external-consumer.ts) separately compiles an interface-input module, a sibling provider facade, and a consumer importing only their public barrel. They share a `unique symbol` through type-only imports from one generated internal module. The concrete provider is accepted; the standalone raw-host negative fixture produces exactly one TS2345. Declaration output retains the shared internal type import, while emitted JavaScript contains no brand import or property. The public barrel and package exports should omit the internal brand. This prevents accidental structural acceptance in ordinary user code; it is not runtime authority or a defense against casts and deep source access.

`Agent.run<Out>(spec: FunctionSpec<Out>): Promise<RunResult<Out>>` infers Resume and Invoice independently on the same Agent. Wrong assignments fail. No manual output token or `.as_runner()` is necessary for a typed spec.

The executable JavaScript model verifies rejection of forged objects, foreign-session refs, repinned decoders, and incorrect host results. It also normalizes immediate/deferred Promise results and exercises reentry and cancellation. Its private WeakMap stands in for trusted bridge state. These are model checks, not production bridge tests; they do not establish native scheduling or lease correctness. The separate observation that `declare` brands emit no symbols is a real TypeScript compilation result.

## Python: real image callbacks

File: [media_callback_roundtrip.py](/Users/aaron/projects/baml/baml_language/interface_probes/python/media_callback_roundtrip.py).

Run from `baml_language/sdks/python`:

```sh
uv run python ../../interface_probes/python/media_callback_roundtrip.py
```

Tested with Python 3.14.3 and the editable `baml_bridge` 0.18.0 built from the current checkout. The probe initializes a real runtime from BAML source, avoiding a stale generated-bytecode fingerprint. It makes no model or network calls.

Two paths pass:

1. A Python-created image travels Python → BAML → Python callback → BAML → Python result.
2. BAML constructs an image, passes it to the Python callback, receives it back, and returns it to Python.

The callback inspects payload and MIME, and returns the received image. The examples use inert base64 payloads to test transport; they do not decode pixels or validate PNG/WebP encoding. Both paths preserve their payload and MIME. Python wrappers have value semantics: the callback/result wrappers are not the original Python object.

Observed live-handle counts:

| Point | Count |
|---|---:|
| Initial baseline | 0 |
| Original Python image retained | 1 |
| Host-origin round-trip result also retained | 2 |
| Result dropped, events flushed, Python GC | 1 |
| BAML-origin round-trip result retained | 2 |
| That result dropped, events flushed, Python GC | 1 |

This directly demonstrates successful BAML-created image callback transfer on the matched checkout. It does not establish the installed versions or issue status of the application reported in B-1683, and it does not test the proposed new media encoding or engine-independent wrappers.

Two narrow existing tests also pass:

```sh
uv run pytest -n 0 tests/test_portable_values.py::test_media_decodes_and_reencodes_as_portable_payload -q
uv run pytest -n 0 tests/test_engine.py -k returned_closure_is_reusable_and_retains_captures -q
```

## Python: a real returned-callable failure

File: [returned_closure_roundtrip.py](/Users/aaron/projects/baml/baml_language/interface_probes/python/returned_closure_roundtrip.py).

Run from `baml_language/sdks/python`:

```sh
uv run python ../../interface_probes/python/returned_closure_roundtrip.py
```

The probe creates a stateful counter closure inside the real BAML runtime. Calling the returned closure twice in Python produces `[41, 42]`. Its captured state survives the factory call. Dropping this directly used wrapper, flushing events, and collecting Python garbage returns the handle count from 1 to the baseline 0.

Passing another returned closure back into a BAML `Apply(callback)` function fails. The current encoder does not take the existing-capability path for `BamlClosure`; it adapts it as a new Python callable. Calling that adapter invokes the closure's synchronous runtime path from a Tokio worker. The native failure is:

```text
Cannot start a runtime from within a runtime
```

The outer Python diagnostic then reports an unknown `baml.panics.SdkPanic` class, masking the primary error. This is a production runtime observation, not a scheduler simulation.

After recording only error text, clearing the exception traceback, deleting the closure, flushing events, and Python GC, one handle remains. The probe has no exposed Python API for forcing BAML VM collection. This is a retained-handle observation, not proof of a permanent leak.

The resulting design requirements are concrete:

- Recognize existing engine-owned callables and object/interface refs before generic callable or structural adaptation.
- Return their original receiver/type information to the owning runtime; do not bind them again as host implementations.
- Use async proxy calls. Reject unsafe synchronous reentry before entering a blocking native path.
- Roll back temporary roots when encoding or call setup fails, and measure retention with exception tracebacks cleared.
- Decode infrastructure errors independently of the application's generated class typemap.

The script deliberately asserts today's failure so the finding can be reproduced. When the bridge is fixed, replace that expectation with successful state-preserving pass-back before promoting it into the permanent conformance suite.

## Python: Pydantic and automatic ownership

File: [pydantic_lifetime.py](/Users/aaron/projects/baml/baml_language/interface_probes/python/pydantic_lifetime.py). Run from `baml_language` using the existing Python SDK environment:

```sh
sdks/python/.venv/bin/python interface_probes/python/pydantic_lifetime.py
```

Tested with Pydantic 2.13.3 and Python 3.14.3. This uses real Pydantic behavior and an isolated ownership model; it does not exercise the production interface ABI.

The model composes `BaseModel` with a generated-style abstract `GreeterImplementation` base. Missing required methods prevent instantiation. Declared fields validate normally, and `model_dump()`/`model_dump_json()` contain only the model's own configuration. `model_copy(deep=True)` makes a distinct equal native instance. A custom interface-input schema retains the original opted-in object without conversion, while rejecting interface JSON input/output. Thus an implementation can export its configuration through its own model API without treating a live interface-typed value as portable data. Automatic engine binding, complete signature checks, and trusted witness creation remain implementation work.

Pydantic instances support weak references but mutable models are unhashable: inserting one into `WeakKeyDictionary` fails. The bridge needs an identity-based cache independent of host equality/hashing, with weak object references, generation checks, concurrent first-bind coordination, and no strong cycle through cached values. Distinct copied/equal models must not become the same BAML receiver.

The ownership model verifies:

- Collecting a Python proxy enqueues release of only its owned lease.
- A separate retained BAML lease keeps the host object alive; releasing that final lease allows collection.
- Explicit `close()` and later garbage collection enqueue exactly one release.
- A host object retaining a proxy whose external registry entry retains that host survives host garbage collection. Clearing the registry during modeled scope/runtime teardown breaks that cycle.

The design now makes Python's explicit scope optional and offers `GreeterImplementation` for direct, explicitly opted-in inputs. Finalization belongs on the proxy/lease, not on the retained user object. It enqueues cleanup without awaiting or running application code. Runtime shutdown must drain/invalidate this path safely; finalizers alone cannot prove timely async resource cleanup or solve cross-runtime cycles. These conclusions agree with Python's [finalizer guidance](https://docs.python.org/3/library/weakref.html#finalizer-objects) and [object lifetime rules](https://docs.python.org/3/reference/datamodel.html#objects-values-and-types).

## TypeScript and Rust: synchronous factories, lazy runtime binding

TypeScript fixture: [ergonomic-inputs.ts](/Users/aaron/projects/baml/baml_language/interface_probes/typescript/ergonomic-inputs.ts), included in the strict compile and documented in the [README](/Users/aaron/projects/baml/baml_language/interface_probes/typescript/README.md).

```sh
node ../node_modules/typescript/bin/tsc -p interface_probes/typescript/tsconfig.json
node interface_probes/typescript/dist/ergonomic-inputs.js
```

With TypeScript 5.8.3, `Greeter.implement(host)` returns a branded implementation source with no runtime binding. The observed bind counts are construction `0`, first call `1`, repeated call `1`, independent wrapper `2`. A raw class implementing `GreeterHost` fails the direct-input type check, and a forged cast fails the runtime model's private-state check. Default `label()` goes through a mock runtime dispatcher; the host supplies no copied default body. Ref pass-back is checked first; a closed ref never falls through to rebinding. Ref clones retain the same receiver independently, while different wrappers/runtimes have separate registrations.

This validates static API shape and modeled dispatch/identity, not the production runtime. The prototype resolves registration synchronously inside its async prepare function. Real asynchronous registration must share one in-progress operation per wrapper/runtime/scope and correctly roll back failures. Default-method calls must select the same SDK runtime as generated free functions; multi-runtime applications need an explicit selection path. No FinalizationRegistry timing is used as evidence of cleanup.

Rust fixture: [automatic_input.rs](/Users/aaron/projects/baml/baml_language/interface_probes/rust/automatic_input.rs), executed by the existing runner:

```sh
python3 interface_probes/rust/run.py
```

Under rustc 1.93.0, `Greeter::implement(H)` creates an Arc-owning implementation wrapper synchronously. It satisfies the same object-safe input trait as a generated facade or checked ref, accepts direct function calls, and exposes a default `label()` that forwards to a mock runtime-owned body. Synchronous input preparation produces either a checked view or an owned host source; only the async invocation binds it. Options can outlive the original application wrapper. Clones retain one receiver identity, and dropping application ownership leaves a mock BAML-retained reference usable; the final release produces exactly one host Drop.

The alternative `Arc<H: GreeterHost>` input also compiles alongside facade/ref implementations. An explicit `Arc<dyn GreeterHost>` implementation handles the erased case; the generic unsized Arc-to-trait-object coercion is not automatically available. Neither form discovers all traits implemented by H. Inferring an interface bundle from whichever parameter is encountered next risks changing an already-published implementation set or producing multiple adapter types for one apparent instance. The model rejects such a descriptor mismatch. Direct Arc method lookup can also expose native methods through dereferencing instead of a generated default forwarder.

The chosen API is therefore the explicit factory wrapper, which fixes both ownership and interface metadata before any runtime call. Its primary contract includes all required interfaces. Supporting multiple unrelated interfaces requires one explicit combined registration, with all checked views derived from that receiver. A future host-type declaration macro could enable direct Arc ergonomics with a complete descriptor; it is not necessary for this design. These are Rust compiler checks and an ownership model, not proof of native BAML lease/cancellation behavior.

## TypeScript: `using`, `await using`, and Node 22

Files: [disposal.ts](/Users/aaron/projects/baml/baml_language/interface_probes/typescript/disposal.ts), [native-disposal-support.mjs](/Users/aaron/projects/baml/baml_language/interface_probes/typescript/native-disposal-support.mjs).

```sh
node ../node_modules/typescript/bin/tsc -p interface_probes/typescript/tsconfig.json
node interface_probes/typescript/dist/disposal.js
node interface_probes/typescript/native-disposal-support.mjs
```

The installed Node is 22.22.2 with V8 12.4.254.21-node.39. Both `Symbol.dispose` and `Symbol.asyncDispose` are symbols. Both raw declaration forms fail parsing with SyntaxError. The native probe reports support rather than asserting continued failure on future Node versions. Node's [24.0.0 release notes](https://nodejs.org/en/blog/release/v24.0.0) identify native explicit resource management as a new feature.

TypeScript 5.8.3, targeting ES2022 with `ESNext.Disposable` in its libs, compiles both forms into helpers and `try/finally`-style cleanup. Executing that emitted JavaScript on this same Node 22 passes. No symbol polyfill was required on the tested runtime; older browsers/runtimes may need one. This agrees with [TypeScript's downlevel support](https://www.typescriptlang.org/docs/handbook/release-notes/typescript-5-2.html).

Real language execution verifies synchronous disposal on normal and throwing exits, reverse-order cleanup, and asynchronous disposal awaited before the enclosing operation completes. The ownership model separately verifies that disposing one proxy releases exactly its lease while a retained receiver remains usable. Modeled scope disposal immediately revokes new calls, awaits existing leased work, and resolves once; repeated close/disposal is idempotent. The structured example awaits its work before disposing the proxy and scope. This does not exercise production host cancellation or native queue races, and no forced GC/finalizer timing is used as evidence.

The design exposes `[Symbol.dispose]()` on refs/owned implementation wrappers and `[Symbol.asyncDispose]()` on bridge scopes/runtimes. Local disposal and registration revocation stay separate operations. Explicit resource management makes timely cleanup convenient; `FinalizationRegistry` remains a fallback for undisposed wrappers.

## Interface fields: scalar visibility and nested mutable identity

BAML fixture: [interface_field_mutability](/Users/aaron/projects/baml/baml_language/interface_probes/baml/interface_field_mutability/baml_src/main.baml), included in `interface_probes/baml/run_probes.sh`. [Full observations](/Users/aaron/projects/baml/baml_language/interface_probes/baml/RESULTS.md:152) were captured with the source-built CLI 0.18.0 at the research HEAD.

The real runtime verifies:

- A concrete scalar write is read through the interface as `2`; an interface write updates the concrete object's linked storage to `3`.
- Aliases read through list, map, and child-class fields preserve identity. Mutating them produces a default-method observation containing first list item `11`, map values `a=11, b=2`, and child value `12`.
- Replacing the root fields makes the default observe a new list `[99]`, map `a=9`, and child value `8`. Further mutation of the old aliases changes those detached objects, leaving the replacement roots untouched.

This agrees with the [VM virtual load/store implementation](/Users/aaron/projects/baml/baml_language/crates/bex_vm/src/vm.rs:8164): a load pushes the stored Value and a store replaces the linked physical slot. Nested mutation need not execute a subsequent virtual store to the parent field. These are in-BAML observations, not proof of foreign mutable storage support.

Python fixture: [pydantic_live_fields.py](/Users/aaron/projects/baml/baml_language/interface_probes/python/pydantic_live_fields.py).

```sh
sdks/python/.venv/bin/python interface_probes/python/pydantic_live_fields.py
```

Using real Pydantic and an isolated async adapter, native `host.count = 2` is observed by the modeled getter; the modeled setter writes `3` into the same native object. Strict validation rejects a string installed through a native validation bypass. The proxy rejects plain assignment instead of allowing a shadow attribute; `await ref.count = 4` is also invalid Python syntax. An invalid element appended in place bypasses Pydantic assignment validation and is caught at the next checked read.

The intentionally rejected aggregate getter deep-copies its list. Appending to that returned list changes only the copy, not the host list. It demonstrates why scalar accessor success is insufficient evidence of complete field semantics. A future direct-field design would require typed shared array/map/class storage views, recursive owner-aware mutations and root replacement that preserves old aliases. This PR defers that entire protocol. Ordinary method data results are copied records/containers, while useful changes run through authored methods at the state owner.

Those new storage backings and generated field projections are outside this PR. The current probes explain why the deferred field protocol cannot be replaced with an implicitly copied getter; they do not demonstrate a foreign-storage backend.

## Stateful methods on a BAML-owned receiver

Fixture: [stateful_methods](/Users/aaron/projects/baml/baml_language/interface_probes/baml/stateful_methods/baml_src/main.baml). This fixture defines a method-only `Counter` interface and a BAML class whose `add` method changes its own `stored_count` field. It uses only the existing runtime; it implements no bridge features.

Executed from `baml_language` with the existing source-built CLI:

```sh
./target/debug/baml-cli --project interface_probes/baml/stateful_methods check
./target/debug/baml-cli --project interface_probes/baml/stateful_methods run observe_stateful_methods --output-format json
./target/debug/baml-cli --project interface_probes/baml/stateful_methods test
```

Checking succeeded and the one BAML test passed. Execution returned:

```json
{"initial":0,"after_method":2,"after_pass_back":5,"through_old_alias":5,"detached_read":2}
```

An interface method mutates the original BAML-owned class, and passing that interface to another BAML function preserves the receiver. An old alias sees the new state; a previously returned integer stays unchanged. The proposed SDK must preserve the same receiver when a host requests those calls. No host round-trip was implemented or proved by this fixture; that remains a bridge acceptance test.

## Remaining validation boundary

The method-capable interface ABI, its scopes, host generic frames and media representation are still proposed. Direct field dispatch/shared mutable storage are deferred. These experiments cannot demonstrate full bridge parity before those paths exist. Scope self-close, cancellation racing a host completion, worker-thread retention after cancellation, cross-runtime cycles, and transactional ownership of partially encoded aggregates remain mandatory implementation tests.

The probes justify specific changes to the design; they do not justify preserving old bugs, layouts, encodings, or wrapper APIs.
