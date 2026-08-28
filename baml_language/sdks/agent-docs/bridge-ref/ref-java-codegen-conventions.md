---
date: 2026-07-15
repository: baml4
---
# Java codegen + test-porting conventions (provisional)

These are the generated-API conventions the JUnit parity tests in
`sdk_tests/crates/java/` are written against. They are **provisional
design commitments** — the tests encode them test-first, and the
`sdkgen_java` emitter must satisfy them (or this doc and the tests
change together). Companion to `ref-java-state-of-completeness.md`.

Since first written, several of these have been **owner-decided**
(D1 cancellation, D2 error mapping) or shipped and parity-tested; each
such rule is tagged **[decided]**. Rules still awaiting the owner are
tagged **[open]** with their options and must **not** be resolved here.
Rationale and the full decided/open log live in the decisions doc
`thoughts/antonio/java-function-calls-decisions.md` (this file is the
rules digest, not the rationale log).

## Generated API shape

- **Package root**: `baml_sdk`. BAML namespaces map to subpackages
  (`user.lorem.Resume` → `baml_sdk.lorem.Resume`, vendor packages →
  `baml_sdk.vendor.aws.s3`, stdlib → `baml_sdk.baml.media`). Child
  symbols are never hoisted into parent packages.
- **Naming**: `NamingConvention::PreserveCase`, matching the TS
  precedent — BAML `return_int` stays `return_int` in Java. Rationale:
  1:1 grep-ability with BAML source and the cross-language parity
  checker outweigh Java camelCase idiom for generated code.
- **Free functions**: static methods on a generated holder class
  `Fns` in the namespace's package: `baml_sdk.primitives.Fns.return_int()`;
  root-namespace functions on `baml_sdk.Fns`. (BAML user symbols could
  collide with `Fns`; if a fixture ever defines one, codegen escapes the
  holder to `Fns$`. Holder name is on the cross-language sync agenda.)
- **Async siblings**: every callable gets `<name>_async` returning
  `java.util.concurrent.CompletableFuture<T>`.
- **Sync/async pairing and LLM operations**: every authored callable gets its
  ordinary `_async` sibling. LLM operations are emitted as flat
  `<name>_spec` / `<name>_spec_async` factories and, when streamable,
  `<name>_stream` / `<name>_stream_async` shortcuts. The spec factory dispatches
  the Spec operation on the authored FQN; the stream shortcut sends that same
  FQN with the Stream boundary operation, which the engine resolves to PPIR's
  private ordinary `<name>@stream` function. Codegen never invents callable
  `$spec`, `$stream`, `$parse`, `$render_prompt`, or `$build_request` names.
- **Classes** **[decided]**: generated as `public final` immutable value
  classes with a canonical all-args constructor (field declaration order)
  and PreserveCase accessor methods (`p.int_field()`). `final` because the
  encoder keys its typemap on the *exact* runtime class — a user subclass
  would silently break inbound-encode (exact-class value semantics). Covers
  plain value classes, generic classes, and PPIR `$stream` partial-output
  classes; sealed
  union interfaces and their permitted records are the exception (records are
  already final). Value equality is deep: `equals`/`hashCode` are generated
  and handle `byte[]` fields via `Arrays.equals` (tests assert whole-object
  equality on round trips). **POJOs, not `record`s** (owner 2026-07-17): a
  record's auto-generated `equals` compares `byte[]` components by reference,
  so the deep `byte[]` equality has to be hand-generated regardless; the
  reified generic type-args live in a **weak-identity side-table** (below),
  not a hidden instance field, so that is not the deciding factor; and a
  future mutability wishlist would foreclose records anyway. Staying with
  generated final POJOs + generated deep equals.
- **Enums**: generated Java `enum` with PreserveCase constants plus a
  wire-name serializer map for non-identifier spellings.
- **Type mappings** (test-visible): BAML `int` → `long`, `float` →
  `double`, `string` → `String`, `bool` → `boolean` (boxed where
  nullability requires), `uint8array` → `byte[]`, `null`-typed values
  → `Void`/`null`, lists → `java.util.List<T>`, maps →
  `java.util.Map<String, V>`, `T?` → boxed nullable `T` (never
  `Optional<T>` in signatures, never a union type).
- **Unions (TEAM DECISION 2026-07-16)** **[decided]**: anonymous multi-arm unions
  render as the runtime library's **generic arity family**
  `baml_bridge.Union2<A,B>` … `Union10<...>` — sealed interfaces with
  nested generic records `Arm0..Arm{n-1}`, one per positional arm in
  BAML declaration order (post-normalization, null arm stripped), so
  Java 21+ consumers get exhaustive `switch` with record patterns
  (`case Union2.Arm0(var x) -> ...`); Java 17 uses `instanceof`.
  Arm selection is **type-directed** (Kai's model): decode matches the
  wire value against the *declared* arm list in source order — the
  wire carries no arm order and none is trusted. Same-base literal
  unions still erase to the base type; arity > 10 falls back to
  `java.lang.Object` until the threshold/alias policy lands. **Recursive type aliases
  keep a minted nominal sealed type named after the alias** (a
  positional generic cannot reference itself); their arms follow the
  same record-naming scheme as before.
- **Type-directed decode descriptors** **[decided]** (shipped): every
  generated binding passes a typed `baml_bridge.BamlType` for its
  declared return type as the last non-`ctx` argument of
  `BamlFfi.callSync/callAsync`, and `registerClass` gains a parallel
  `baml_bridge.BamlType[] fieldDescs`. The descriptor is a **data
  structure, not a string** — the emitter renders `BamlType` builder
  expressions (`descriptor_expr`, translate_ty.rs), pooled into a
  per-holder `private static final baml_bridge.BamlType $RET{n}`
  constant and referenced by name. Spelling: primitives
  `BamlType.INT|STRING|BOOL|FLOAT`; a class/enum/named-recursive-alias →
  `BamlType.classByFqn("<baml fqn>")` (bare FQN — the registry resolves
  the kind); `BamlType.list(D)`; `BamlType.map(BamlType.STRING, D)`; an
  ordered anonymous union `BamlType.union(D, D, …)` (declaration order);
  a literal `BamlType.literalString("…")` / `literalInt(…L)` / … (raw
  value — no escaping); a TypeVar `BamlType.typeVar("<name>")` and the
  wildcard `BamlType.UNKNOWN` are **decode-only** hints (they throw on
  `toWireTy` — never encoded) and decode wire-driven. A wholly
  wire-driven return passes the literal `null` (also the streaming
  mode). Registration keys the union registry structurally on the
  **arm set** (a sorted, distinct `List<BamlType>` — value equality, no
  rendered key); the runtime derives the equal arm set from the wire
  `self_type` (`ProtoReader.wireArmType`). Decode without a descriptor,
  or on any descriptor/value mismatch with a self-describing wire form,
  falls back to the wire-driven path (error/panic values stay
  wire-driven). *(History: this replaced an earlier stringly-typed
  descriptor grammar — `union[a;b]` / `list<int>` / `lit:string:draft`
  with percent-escaping — and its hand-rolled parser, killed 2026-07.)*
- **Optional args** **[decided]** (shipped, TestOptionalArgs 4/4):
  AWS-SDK-v2-style trailing configurator overload:
  `Fns.optional_args_probe(1)` omits everything (engine evaluates BAML
  defaults); `Fns.optional_args_probe(1, o -> o.opt1(5))` supplies
  values. Calling `o.opt1(null)` sends an explicit BAML `null`; not
  calling the setter leaves the arg UNSET/omitted. This preserves the
  omit-vs-null tri-state without a sentinel type. The configurator is a
  trailing `Consumer<<Ident>$Opts>` and the `$Opts` bag nests under the
  callable's holder (`emit.rs:506-528`). Each entry-point form (required-
  only, and required+configurator) gets its own trailing-`ctx` overload
  pair for cancellation (see below), so `ctx` is always the last
  parameter.
- **Cancellation & call context** **[decided]** (D1, Design B — shipped,
  TestCancellation 7/7): a public `baml_bridge.BamlCallContext` mirrors
  `bridge_python`'s semantics (aborted latch + active-call-id list,
  `abort()` cancels every bound id and is idempotent, attach-while-
  aborted cancels immediately, one ctx may govern several concurrent
  calls — `BamlCallContext.java`). Every binding gets a **trailing-`ctx`
  overload** `f(req.., ctx)` / `f(req.., opts, ctx)` / `f_async(...)`
  (`ctx` always last, threaded as the last runtime-call argument —
  `emit.rs:470-504`, `render_method_pair` with `with_ctx`). Surfacing:
  - **async engine-driven abort** → `BamlCancelledError extends
    CancellationException` (`BamlCancelledError.java:26`), so the future
    reports `isCancelled() == true` and `join()`/`get()` throw it
    **directly, unwrapped**; the decoded `baml.panics.Cancelled` rides on
    `.value()`. The async remap is layered on the shared decode
    (`BamlFfi.mapAsyncFailure`, `BamlFfi.java:428`) — sync never calls it.
  - **`future.cancel(true)`** → fires `cancel_function_call(callId)`
    engine-side, then marks the future cancelled → a **raw**
    `CancellationException` (`CancellableCall.cancel`, `BamlFfi.java:365`).
  - **sync abort** → stays `BamlPanic` whose `.value()` is a `Cancelled`.
  - Implementation-refined: `callAsync` returns the cancel-owning future
    itself (`CancellableCall` holding its `call_id`), not a derived
    stage, so `cancel` actually reaches the engine call. On **JDK 19+**
    the base `CompletableFuture` re-wraps a stored `CancellationException`
    at report time, so `CancellableCall` overrides `join()`/`get()`/
    `getNow()` to **unwrap** the `BamlCancelledError` back out
    (`BamlFfi.java:371-417`), preserving the decided contract across JDKs.
    New JNI export `nativeCancelFunctionCall` wraps
    `bridge_cffi::cancel_function_call_by_id` (`BamlFfi.java:118`).
- **Explicit generics — `BamlType` / `BamlTypes`** (D3): the runtime
  substrate is **[decided]** and shipped (no generated surface reaches it
  yet); two questions on the emitter surface stay **[open]**.
  - **[decided]** Call-site binding is a **named bag**
    `BamlTypes.of("T", BamlType.INT).and("U", …)` passed as a trailing
    overload arg — 1:1 with the wire's named `BamlTyArg` bindings, partial
    binding allowed, duplicate name rejected, insertion (De Bruijn) order
    preserved: enclosing-class params first, then the callee's own
    `<…>` params (`BamlTypes.java`).
  - **[decided]** Token grammar is **minimal**: primitive constants
    `BamlType.INT/STRING/BOOL/FLOAT`, `of(Class)` for a registered
    generated class/enum, `of(Class, BamlType…)` for a reified generic
    class; value equality (`BamlType.java`). `of(Class)` is **bimodal on
    the wire** — enums lower to `BamlTy.enum`, classes to
    `BamlTy.class_ty` (mirrors Python's `_fill_wire_ty`,
    `BamlType.java:93-106`).
  - **[decided]** Wire: the bag encodes as `CallFunctionArgs.type_args`;
    when absent the output is **byte-identical** to the pre-generics
    encoder (`ProtoWriter.encodeCallFunctionArgs`,
    `ProtoWriter.java:110`). The 6-arg `callSync`/`callAsync` that thread
    the bag are **package-private and not yet reached from generated
    code** — the emitter surface is deferred (`BamlFfi.java:213`, `290`).
  - **[decided]** Decode: an outbound `class_value` carrying `type_args`
    retains its reified tokens in a **weak-identity-keyed** side-table
    (`TypeRegistry.typeArgsOf`/`bindTypeArgs`,
    `TypeRegistry.java:315-362`; identity-keyed because value-class
    records compare equal, so a `WeakHashMap` would collide distinct
    instances). Binding is **all-or-nothing** to keep De Bruijn positions
    aligned: because the token grammar is minimal, a reified arg that is a
    list/map/union/optional/literal produces **no** side-table entry (a
    nested out-of-grammar arg poisons the whole token —
    `BamlType.classFromWire`, `BamlType.java:229-231`).
  - **[decided]** D3 readback (owner, 2026-07-17): per-instance
    `bamlTypeArgs()` delegating to the side-table, with the `Fns$`-style
    yield-to-user escape (`bamlTypeArgs$()`) iff a BAML field claims the
    name. Reified factories `of(BamlType…, fields…)` bind on construct.
  - **[decided]** D3 overloads (owner, 2026-07-17): trailing-overload
    matrix, fixed order `f(required…, opts?, types?, ctx?)`; only
    combinations that exist for the callable are emitted (worst case 16
    methods for generic+optional). Synthetic param names (`types`, `ctx`)
    yield to user arguments via trailing-`$` escape.
  - **[decided]** token grammar (owner: "do B now"): FULL —
    list/map/optional/union/literal tokens with wire round-trip; the
    side-table binds whenever every wire arg is representable (residual
    skips: media/function/rust_type/etc. arms and null/bytes primitive
    kinds, still all-or-nothing to keep positions aligned).
  - Known wart: `BamlType.toWireTy()`/`fromWireTy` are `public` only
    because the codec lives in `baml_bridge.internal`; hiding them needs
    a package reshuffle.
- **Runtime init** **[decided]**: (idempotent) runtime initialization from
  embedded bytecode runs from a static initializer on the root `Baml` anchor —
  the Java analog of Python's root-package import side effect. Note the Java
  semantics: a bare `.class` reference does **not** run a class's static
  initializer, so the runtime boots only on genuine class *initialization* —
  `Baml.ensure()`, `Class.forName(name, /*initialize=*/true, cl)`, `new`, or a
  static-member touch. Every generated `Fns` holder forces this via a
  `static { <root>.Baml.ensure(); }` block, and any generated class that carries
  static/instance method bindings emits the same block so its first invoked
  entrypoint (e.g. `Greeter.create()`) boots the runtime. The anchor name is
  `Baml`, or `Baml$` when a user root-level type already claims `Baml`.
  `nativeInitFromBytecode` also
  **registers the bridge with the versioned C ABI** —
  `BridgeLanguage::Java = 7` (telemetry id `"java"`) at
  `baml_version::CANONICAL_VERSION`, mirroring `bridge_python`
  (`bridge_java/src/lib.rs:118`); a canonical-version mismatch surfaces as
  a Java exception.
- **Streams** **[decided]** (OWNER, 2026-07-18; landed): `BamlStream<TPartial,
  TFinal>` runtime wrapper (`baml_bridge`) with `next()` / `next_async()` /
  `get_final()` / `get_final_async()`. Python spells the last pair `final()` /
  `final_async()`, but `final` is a Java reserved word — the getter is
  **`get_final`** (an explicit OWNER override of the `$`-escape default, i.e.
  NOT `final$()`). `next()` returns "partial or finished-sentinel" as
  `Object` + `instanceof Done` (Python's sentinel duck-typing;
  a sealed `StreamItem<T>` stays a possible future refinement — deferred, not
  blocking). The finished sentinel is **`baml_sdk.ai.stream.Done`**,
  **runtime-owned like the media classes** (its body ships in `baml-bridge`; the
  emitter's `RUNTIME_OWNED_FQNS` skips generating a base `Done.java`) and
  **registered in the typemap under its BAML FQN** (`ai.stream.Done`) by a
  `TypeRegistry` static block, so a `class_value(ai.stream.Done, {})` decodes to
  it. Exhaustion follows Python: `next()` returns partial values until it
  returns a `Done` VALUE — no `null`, no exception. On the wire a `BamlStream`
  is a bare `handle_value(ADT_TAGGED_HEAP_HANDLE)` whose outbound `ty` carries
  the concrete stream class FQN. Decode retains that identity and derives
  `<FQN>.next` / `<FQN>.final`; encode clones the receiver key per the drain
  contract.
- **`$stream` partial-model packaging** **[decided]** (owner, 2026-07-17;
  `BamlStream` itself untouched — this is only where host-constructible
  partial-model classes live): in-package `$`-preserved PPIR models —
  `<ns>.<Name>$stream` beside its base type, exactly as emitted and
  registered today. This matches TS (`Resume$stream` beside `Resume`, no
  `stream_types/` tree; the compiler no longer reserves `stream_types` —
  ref-ts-type-mappings.md:8,51,61). Python's parallel `stream_types.*` package
  is a workaround for `$` being illegal in Python identifiers and is NOT
  mirrored; the ported stream tests retarget to the generated layout.
  Independent open question flagged by the test authors: whether the
  engine ACCEPTS a host-constructed partial through a
  `$stream`-typed param — a red there is a bridge-surface limitation, not
  a test bug.
- **Native env hook** **[landed]**: the replay harness needs the *engine's* view of the
  environment mutated at runtime (Python uses `os.environ`, which the
  in-process engine reads). JVM-side env patching (junit-pioneer's
  `@SetEnvironmentVariable`) does not reach native `getenv` — and throws
  `InaccessibleObjectException` on JDK 17+ — so `bridge_java` exposes a real
  native setenv shim via `baml_bridge.BridgeEnv.set/unset` (JNI →
  `std::env::set_var`/`remove_var`, the same process `environ` the engine's
  `std::env::var` reads). The ported tests call it directly (bracketed with
  `try/finally` to restore).
- **Errors** **[decided]** (D2 — shipped, 1:1 with Python's reference
  bridge; full mapping also in the state-of-completeness doc):
  - `baml.errors.TypeMismatch` is the **only** class→native remap:
    `decodeError` throws `IllegalArgumentException` carrying the value's
    `message` field (mirrors Python's `TypeError` remap) — the caller-bug
    arm the generics tests rely on (`ProtoReader.java:239-252`).
  - BAML stack frames are synthesized into **real `StackTraceElement`s**
    and prepended onto the native exception (`BamlTraceback.splice`, same
    trace-line regex as Python; wire frames reversed, dotted functions
    split namespace/leaf, namespace-less frames get the `<baml>` sentinel
    — `BamlTraceback.java`). Best-effort: a malformed line is skipped and
    delivery never depends on the splice. Applied to the remapped IAE,
    `BamlError`, `BamlPanic`, and `BamlCancelledError`.
  - **`BamlPanic extends Error`** (not `RuntimeException`) — the analog of
    Python's `BamlPanic` subclassing `BaseException`: a bare
    `catch (Exception)` no longer swallows a panic
    (`BamlPanic.java:20`). Callers intercept a panic via `catch (BamlPanic)`
    or `catch (Throwable)`. A clean `baml.sys.exit` never reaches here —
    the decoder halts the process via `Runtime.getRuntime().halt(code)`.
  - `BamlError` / `BamlPanic` accessors are snake_case (`baml_trace()`,
    `class_name()`) for 1:1 cross-language parity.
- **`@throws` Javadoc** **[decided]** (shipped, TestRaises 8/8): a
  callable's thrown-type contract renders as one `@throws <Name>` tag
  per thrown type — **leaf/unqualified name, source order, de-duped**; a
  union throws renders **one tag per arm**; inferred contracts are
  included; and the sync binding, its `_async` sibling, and every
  configurator/`ctx` overload **share the same tags**
  (`collect_raises_names`, `emit.rs:110-135`; rendered at
  `emit.rs:434-439`). There are no checked exceptions on the JVM side.
- **Handle-backed value encode** **[decided]** (shipped): when a
  handle-backed value is encoded inbound, its key is **freshly cloned**
  (`BamlHandle.cloneKeyForWire`, `BamlHandle.java:120`) because the engine
  **drains** the sent key on decode while the Java object keeps its own.
  This covers both a media value's `_data` handle
  (`class_value(baml.media.X)`) and a bare `$rust_type` shell's private
  handle field (`baml.fs.File._handle`, `baml.http.Response._body` →
  `InboundValue.handle{key, handle_type}`) — same drain contract for both
  (`ProtoWriter.java:200-210`, `259-260`).
- **Host callables** **[open]** (TestHostCallables — nothing in place
  Java-side yet; the whole slice, decisions A–F, awaits the owner). Load
  the decisions doc for the end-to-end Python reference and the option
  write-ups. In brief, still-open choices: (A) callback registration +
  registry location (Java-side `ConcurrentHashMap<Long,Object>` vs
  Rust-side `GlobalRef`); (B) dispatch executor/threading (dedicated
  cached pool vs inline on a tokio worker); (C) async-callable detection
  (detect `CompletableFuture` in the result handler vs a typed `*Async`
  overload vs sync-only); (D) exception-identity mechanics for
  `assertSame` round-trips; (E) `IntOptCallback`-style generated
  `@FunctionalInterface` + non-null nested `Opts` bag for
  optional/high-arity callables (replacing the current `java.lang.Object`
  fallback in `translate_callable`); (F) 1-arg `BamlError(Object value)`
  throw-direction ctor. Do not resolve here.

## Test-porting rules (Python → JUnit 5)

- File mapping: `roundtrip_tests/test_primitives.py` →
  `roundtrip_tests/TestPrimitives.java` declaring
  `package roundtrip_tests;`. Top-level `test_main.py` →
  `TestMain.java` in the default package (files sit at the Gradle test
  source root `tests/`).
- **Test method names are kept byte-identical** to the Python test
  function names (`@Test void test_round_trip_int()`), snake_case and
  all — the automated suite-comparison checker aligns on names.
- Same cases, same inputs, same assertion strength. Python kwarg call
  sites become positional Java arguments in declared order.
- Literals: `None` → `null`; `b"\x00\x01"` → `new byte[] {0, 1}`;
  Python int/float literals get `L`/`.0` as needed for `long`/`double`.
- Assertions: pytest `assert x == y` → `assertEquals(expected, actual)`
  (JUnit argument order), `is None` → `assertNull`, `is True/False` →
  `assertTrue`/`assertFalse`, `pytest.raises(E)` →
  `assertThrows(E.class, () -> ...)`, byte[] equality →
  `assertArrayEquals`.
- Async variants: `await fn_async(...)` → `Fns.fn_async(...).join()`.
- **Cancellation (Design B, now the convention)**: Python's `_ctx=ctx`
  becomes the trailing `BamlCallContext` overload `Fns.F(args, ctx)` /
  `Fns.F_async(args, ctx)`. An engine-driven `ctx.abort()` on an async
  call is asserted **directly** —
  `assertThrows(BamlCancelledError.class, future::join)` **plus**
  `assertTrue(future.isCancelled())` (it is a `CancellationException`, so
  it is *not* wrapped in `CompletionException`), with the decoded value
  read via `reason.value()` (a `Cancelled`). `future.cancel(true)` is
  asserted as a raw `CancellationException`. A **sync** abort stays
  `assertThrows(BamlPanic.class, ...)` with `exc.value()` a `Cancelled`.
  `asyncio.TaskGroup` sibling cancellation is modeled manually via
  `whenComplete`; `asyncio.wait_for(timeout)` →
  `future.get(t, MILLISECONDS)` → `TimeoutException`. (Reference:
  `function_calls/customizable/TestCancellation.java`.)
- Python-isms with no Java analog (runtime `isinstance` checks after
  static widening, pydantic construction-time coercion): keep the test
  name and intent, adapt the body, and leave a
  `// java-port note: ...` comment explaining the semantic shift.
- Namespace-import smoke tests: the Java analog of `import baml_sdk.ns` is
  compile-time reachability of a known generated symbol. A bare `.class`
  literal only pins reachability — it does **not** run the class's static
  initializer (no class-load side effect), so a test that means to force the
  runtime-init side effect must actually *initialize* the class:
  `Class.forName(name, /*initialize=*/true, cl)`, `Baml.ensure()`, or a
  static-member touch.
