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
- **Sync/async pairing, companions**: `$` is legal in Java
  identifiers, so `$`-companions keep the BAML name verbatim:
  `extract_resume$build_request`, `extract_resume$stream_async`. (Same
  as TS; no `__` mangling.)
- **Classes** **[decided]**: generated as `public final` immutable value
  classes with a canonical all-args constructor (field declaration order)
  and PreserveCase accessor methods (`p.int_field()`). `final` because the
  encoder keys its typemap on the *exact* runtime class — a user subclass
  would silently break inbound-encode (exact-class value semantics). Covers
  plain value classes, generic classes, and `$stream` companions; sealed
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
  generated binding passes a descriptor string for its declared return
  type as the last non-`ctx` argument of `BamlFfi.callSync/callAsync`
  (`emit.rs:429`, threaded at `emit.rs:486`), and `registerClass` gains
  a parallel `String[] fieldDescs`. Descriptor grammar (one tokenizer,
  shared with the emitter): primitives `int|bigint|float|string|bool|
  null|uint8array|void|unknown`; class/enum/named-recursive-alias →
  canonical BAML FQN (registry resolves the kind); `list<D>`;
  `map<D,D>`; ordered anonymous union `union[D;D;...]`;
  `lit:<base>:<value>`; unresolved type vars `tv:<name>` (decoder
  falls back to wire-driven `self_type` decode). Decode without a
  descriptor, or on any descriptor/value mismatch that has a
  self-describing wire form, falls back to the wire-driven path
  (error/panic values stay wire-driven).
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
  - **[open]** D3 readback naming: the accessor is provisionally
    `typeArgsOf`; the emitted per-instance form is `bamlTypeArgs()` with a
    `Fns$`-style collision escape **vs** an always-`$`-named accessor.
  - **[open]** D3 overload matrix: the trailing-overload combinatorics
    (req → opts → types → ctx, worst case 16 methods) **vs** a fluent
    builder. Do not add the `type_args` overloads to the emitter until
    this is decided.
  - Known wart (relevant to the surface ruling): `BamlType.toWireTy()` /
    `fromWireTy` are `public` only because the codec lives in
    `baml_bridge` rather than `baml_bridge.internal`; hiding them needs a
    package reshuffle. And the minimal grammar **caps readback** —
    `bamlTypeArgs()` will either widen the grammar or document graceful
    degradation (folded into the two open items above).
- **Runtime init** **[decided]**: loading any generated class triggers
  (idempotent) runtime initialization from embedded bytecode via a static
  initializer on the root holder — the Java analog of Python's
  root-package import side effect. `nativeInitFromBytecode` also
  **registers the bridge with the versioned C ABI** —
  `BridgeLanguage::Java = 6` (telemetry id `"java"`) at
  `baml_version::CANONICAL_VERSION`, mirroring `bridge_python`
  (`bridge_java/src/lib.rs:118`); a canonical-version mismatch surfaces as
  a Java exception.
- **Streams** **[open]**: `BamlStream<TPartial, TFinal>` runtime wrapper
  with `next()` / `next_async()` / `get_final()` / `get_final_async()`.
  Python spells the last pair `final()` / `final_async()`, but `final`
  is a Java reserved word — `get_final` is the provisional escape
  (alternative: `final$()`, matching the `$`-escape used elsewhere;
  decide before the stream capability lands). `next()` returning
  "partial or finished-sentinel" wants a sealed `StreamItem<T>` rather
  than `Object` + `instanceof StreamFinished` — also to be decided;
  the sentinel's home (`baml_bridge` vs `baml_sdk.baml.stream`) is
  open too.
- **`$stream` partial-model packaging** **[decided]** (owner, 2026-07-17;
  `BamlStream` itself untouched — this is only where host-constructible
  partial-model classes live): in-package `$`-preserved companions —
  `<ns>.<Name>$stream` beside its base type, exactly as emitted and
  registered today. This matches TS (`Resume$stream` beside `Resume`, no
  `stream_types/` tree; the compiler no longer reserves `stream_types` —
  ref-ts-type-mappings.md:8,51,61) and the "`$`-companions keep the BAML
  name verbatim" house rule. Python's parallel `stream_types.*` package
  is a workaround for `$` being illegal in Python identifiers and is NOT
  mirrored; the ported stream tests retarget to the generated layout.
  Independent open question flagged by the test authors: whether the
  engine ACCEPTS a host-constructed partial through a
  `$stream`-typed param — a red there is a bridge-surface limitation, not
  a test bug.
- **Native env hook**: the replay-harness tests need the *engine's*
  view of the environment mutated at runtime (Python uses
  `os.environ`, which the in-process engine reads). JVM-side env
  patching (junit-pioneer) does not reach native `getenv`, so
  `bridge_java` must expose a real native setenv shim — placeholder
  spelled `baml_bridge.BridgeEnv.set/unset` in the ported tests.
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
- Namespace-import smoke tests: the Java analog of `import
  baml_sdk.ns` is referencing a known generated symbol's `.class`
  (compile-time reachability + class-load side effects).
