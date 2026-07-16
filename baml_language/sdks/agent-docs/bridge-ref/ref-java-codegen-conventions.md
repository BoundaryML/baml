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
- **Classes**: generated as immutable value classes with a canonical
  all-args constructor (field declaration order) and PreserveCase
  accessor methods (`p.int_field()`). Value equality is deep:
  `equals`/`hashCode` must handle `byte[]` fields via `Arrays.equals`
  (tests assert whole-object equality on round trips). Whether these
  are `record`s or emitted classes is an emitter detail — records
  can't hold hidden handle/type-args fields, so handle-backed and
  generic classes at least will be ordinary final classes.
- **Enums**: generated Java `enum` with PreserveCase constants plus a
  wire-name serializer map for non-identifier spellings.
- **Type mappings** (test-visible): BAML `int` → `long`, `float` →
  `double`, `string` → `String`, `bool` → `boolean` (boxed where
  nullability requires), `uint8array` → `byte[]`, `null`-typed values
  → `Void`/`null`, lists → `java.util.List<T>`, maps →
  `java.util.Map<String, V>`, `T?` → boxed nullable `T` (never
  `Optional<T>` in signatures, never a union type).
- **Unions (TEAM DECISION 2026-07-16)**: anonymous multi-arm unions
  render as the runtime library's **generic arity family**
  `baml_bridge.Union2<A,B>` … `Union10<...>` — sealed interfaces with
  nested generic records `Arm0..Arm{n-1}`, one per positional arm in
  BAML declaration order (post-normalization, null arm stripped), so
  Java 21+ consumers get exhaustive `switch` with record patterns
  (`case Union2.Arm0(var x) -> ...`); Java 17 uses `instanceof`.
  Arm selection is **type-directed** (Kai's model): decode matches the
  wire value against the *declared* arm list in source order — the
  wire carries no arm order and none is trusted. Same-base literal
  unions still erase to the base type; arity > 10 is a codegen error
  until the threshold/alias policy lands. **Recursive type aliases
  keep a minted nominal sealed type named after the alias** (a
  positional generic cannot reference itself); their arms follow the
  same record-naming scheme as before.
- **Type-directed decode descriptors**: every generated binding passes
  a descriptor string for its declared return type as the last
  argument of `BamlFfi.callSync/callAsync`, and `registerClass` gains
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
- **Optional args**: AWS-SDK-v2-style trailing configurator overload:
  `Fns.optional_args_probe(1)` omits everything (engine evaluates BAML
  defaults); `Fns.optional_args_probe(1, o -> o.opt1(5))` supplies
  values. Calling `o.opt1(null)` sends an explicit BAML `null`; not
  calling the setter leaves the arg UNSET/omitted. This preserves the
  omit-vs-null tri-state without a sentinel type.
- **Runtime init**: loading any generated class triggers (idempotent)
  runtime initialization from embedded bytecode via a static
  initializer on the root holder — the Java analog of Python's
  root-package import side effect.
- **Streams**: `BamlStream<TPartial, TFinal>` runtime wrapper with
  `next()` / `next_async()` / `get_final()` / `get_final_async()`.
  Python spells the last pair `final()` / `final_async()`, but `final`
  is a Java reserved word — `get_final` is the provisional escape
  (alternative: `final$()`, matching the `$`-escape used elsewhere;
  decide before the stream capability lands). `next()` returning
  "partial or finished-sentinel" wants a sealed `StreamItem<T>` rather
  than `Object` + `instanceof StreamFinished` — also to be decided;
  the sentinel's home (`baml_bridge` vs `baml_sdk.baml.stream`) is
  open too.
- **Native env hook**: the replay-harness tests need the *engine's*
  view of the environment mutated at runtime (Python uses
  `os.environ`, which the in-process engine reads). JVM-side env
  patching (junit-pioneer) does not reach native `getenv`, so
  `bridge_java` must expose a real native setenv shim — placeholder
  spelled `baml_bridge.BridgeEnv.set/unset` in the ported tests.
- **Errors**: unchecked `BamlError` / `BamlPanic` (see
  state-of-completeness doc for the full mapping).

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
- Python-isms with no Java analog (runtime `isinstance` checks after
  static widening, pydantic construction-time coercion): keep the test
  name and intent, adapt the body, and leave a
  `// java-port note: ...` comment explaining the semantic shift.
- Namespace-import smoke tests: the Java analog of `import
  baml_sdk.ns` is referencing a known generated symbol's `.class`
  (compile-time reachability + class-load side effects).
