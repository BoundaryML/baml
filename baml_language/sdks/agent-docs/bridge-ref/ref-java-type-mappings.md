---
date: 2026-07-20
repository: baml4
---
These are the rules that dictate how Java SDK generation works for BAML.

**This document mirrors [`ref-python-type-mappings.md`](./ref-python-type-mappings.md)
section-for-section and row-for-row, for a 1:1 side-by-side decision review.** Read the two
files with the Python doc on the left. Every BAML-type row the Python doc documents has a
Java answer here.

**Ground truth.** The Java type translation lives in
`sdks/java/sdkgen_java/src/translate_ty.rs` (`translate_ty` / `translate_union` /
`translate_callable` / `descriptor_expr` / `registry_arm_expr` / `union_arm_token` /
`collect_type_vars`) and `sdks/java/sdkgen_java/src/lib.rs`. Rows cite line ranges. Every
mapping was cross-checked against real generated output under
`sdk_tests/crates/java/*/generated/baml_sdk/**`.

**Conventions used below:**

- Java allows fully-qualified names in every type position, so every rendered expression is an
  FQN and there is no import-collection machinery (translate_ty.rs:1–7).
- **Position matters.** `TyPosition::TopLevel` = field / parameter / return position;
  `TyPosition::Boxed` = generic type argument or a nullable slot. A primitive is `long` /
  `double` / `boolean` at top level but `java.lang.Long` / `…Double` / `…Boolean` when boxed —
  **boxing IS the nullability signal** (a boxed `Long` field is nullable, an unboxed `long` is
  not), because no `@Nullable` annotation dependency has been decided yet (translate_ty.rs:11–16,
  `primitive` at :166–171). Where the two positions render identically (already-reference types),
  the columns show the same value.
- **descriptor** = the type-directed decode descriptor the generated binding hands to
  `BamlFfi.callSync/callAsync` (last positional arg) and that `registerClass` carries per field —
  a typed `baml_bridge.BamlType` **data structure** (not a string), produced by `descriptor_expr`
  (translate_ty.rs) as a builder expression. A wholly wire-driven type (bigint / uint8array / null /
  void / media / callable / handle / the `unknown`-family) passes the literal **`null`** (decode
  wire-driven); a `TypeVar` passes `BamlType.typeVar("T")` and the union-with-a-TypeVar-arm case
  passes `null`.
- `> ⚠ **Deviation from Python:** …` flags every divergence from the Python bridge.
- `**NOT YET IMPLEMENTED IN JAVA**` marks a mapping that is not built yet, with the decided/open
  status pulled from `thoughts/antonio/java-function-calls-decisions.md`.

# Example generated code

Given the same BAML code the Python doc uses:
```
// user's package, in namespace `lorem`
// fully qualified BAML symbol: user.lorem.Resume (root is a reserved pkg name in BAML)
class Resume
class Resume$stream  // bamlc-generated companion type; consumed as a regular TIR class
function extract_resume() -> Resume
function extract_resume$build_request() -> baml.http.Request  // companion, LLM-backed fn

// user's package, in namespace `ipsum`
// fully qualified BAML symbol: user.ipsum.Sentiment
enum Sentiment
function classify_sentiment() -> Sentiment
function classify_sentiment$build_request() -> baml.http.Request

// `aws` package, in namespace `s3`   → fully qualified BAML symbol: aws.s3.Bucket
class Bucket
function create_bucket() -> Bucket

// `baml` package aka standard library, in namespace `http`  → baml.http.Response
class Response
function fetch(url: string) -> Response

// `baml` package aka standard library, in namespace `media`
class Pdf
  function from_url(url: string) -> Pdf

// `baml` package aka standard library, in namespace `io`
class File
  function open() -> File
  function close(self)
```

We'll generate these Java symbols. Free functions have no Java analog, so they become **static
methods on a per-namespace `Fns` holder class**; classes/enums are generated types; stdlib
`class`-scoped functions become static/instance methods on the generated class.
```
                          // types and functions

                          // user code
                    class  baml_sdk.lorem.Resume
            static method  baml_sdk.lorem.Fns.extract_resume()                      -> Resume
            static method  baml_sdk.lorem.Fns.extract_resume_async()                -> CompletableFuture<Resume>

            static method  baml_sdk.lorem.Fns.extract_resume$stream()
            static method  baml_sdk.lorem.Fns.extract_resume$stream_async()

            static method  baml_sdk.lorem.Fns.extract_resume$build_request()
            static method  baml_sdk.lorem.Fns.extract_resume$build_request_async()

                     enum  baml_sdk.ipsum.Sentiment
            static method  baml_sdk.ipsum.Fns.classify_sentiment()
            static method  baml_sdk.ipsum.Fns.classify_sentiment_async()
            static method  baml_sdk.ipsum.Fns.classify_sentiment$build_request()
            static method  baml_sdk.ipsum.Fns.classify_sentiment$build_request_async()

                          // other package
                    class  baml_sdk.vendor.aws.s3.Bucket
            static method  baml_sdk.vendor.aws.s3.Fns.create_bucket()
            static method  baml_sdk.vendor.aws.s3.Fns.create_bucket_async()

                          // standard library, static methods, instance methods
                    class  baml_sdk.baml.http.Response
            static method  baml_sdk.baml.http.Fns.fetch()
            static method  baml_sdk.baml.http.Fns.fetch_async()

                    class  baml_sdk.baml.media.Pdf         (runtime-owned re-export, not codegen'd)
     static method        baml_sdk.baml.media.Pdf.from_url()
     static method        baml_sdk.baml.media.Pdf.from_url_async()

                    class  baml_sdk.baml.io.File
     static method        baml_sdk.baml.io.File.open()
     static method        baml_sdk.baml.io.File.open_async()
    instance method       baml_sdk.baml.io.File.close(...)         // self = receiver (required param 0)
    instance method       baml_sdk.baml.io.File.close_async(...)

                          // companion ($stream) types — IN-PACKAGE `$` companions
                    class  baml_sdk.lorem.Resume$stream
                    class  baml_sdk.vendor.aws.s3.Bucket$stream
                    class  baml_sdk.baml.io.File$stream
                    class  baml_sdk.baml.http.Response$stream
                    class  baml_sdk.baml.media.Pdf$stream
```
Every callable also gets trailing `baml_bridge.BamlCallContext ctx` overloads
(`f(req…, ctx)` / `f(req…, opts, ctx)`) for cancellation.

> ⚠ **Deviation from Python:** BAML free functions become **static methods on a `Fns` holder**
> (`baml_sdk.lorem.Fns.extract_resume()`; root-namespace functions on `baml_sdk.Fns`; holder
> escapes to `Fns$` on a user-symbol collision) because Java has no module-level free functions.
> Python emits them as module-level `def` / `async def`.

> ⚠ **Deviation from Python:** `$`-companions keep the BAML name **verbatim** — `$` is a legal
> Java identifier char — so Java emits `extract_resume$build_request`, `extract_resume$stream`,
> `Resume$stream`. Python mangles: `$stream` → `_stream`, `$build_request` → `__build_request`.
> (Same house rule as TS; ref-java-codegen-conventions.md:30–33.)

> ⚠ **Deviation from Python (DECIDED 2026-07-17, Option B — TS-aligned):** `$stream` companion **types** are minted **in
> package** as `baml_sdk.<ns>.<Name>$stream` (fully emitted, registered, relied on by the
> 102/102 `type_shapes` typemap). Python routes them to a **parallel** `baml_sdk.stream_types.<ns>.<Name>`
> package — the owner-decided layout (TS emits companions in place and the compiler
> no longer reserves `stream_types`; Python's parallel package is a workaround for
> `$` being illegal there). The ported stream tests were retargeted accordingly.

## Exhaustive Ty conversions

Java SDK codegen consumes the codegen-facing `Ty` (a re-export of `baml_type::CodegenTy`), same
as Python. The first column names the upstream TIR shape; the second names the codegen variant
`translate_ty` actually matches. `Ty::Class` / `Ty::Enum` / `Ty::TypeAlias` route to the generated
`baml_sdk/` leaf via `route()`; `$stream` class references route to the in-package `<Name>$stream`
companion (see the decided `$stream` deviation above).

Column key: **Java @ TopLevel** = field/param/return; **Java @ Boxed** = generic type-arg /
nullable slot; **descriptor** = the typed `baml_bridge.BamlType` (or `null` = wire-driven).

| tir-ty | codegen-ty | Example BAML | Java @ TopLevel | Java @ Boxed (type-arg / nullable) | descriptor (BamlType) | translate_ty.rs |
| --- | --- | --- | --- | --- | --- | --- |
| `Ty::Primitive(Int)` | `Ty::Int` | `age int` | `long` | `java.lang.Long` | `BamlType.INT` | :85 |
| `Ty::Primitive(Bigint)` | `Ty::Bigint` | `value bigint` | `java.math.BigInteger` | `java.math.BigInteger` | `null` (wire-driven) | :86 |
| `Ty::Primitive(Float)` | `Ty::Float` | `score float` | `double` | `java.lang.Double` | `BamlType.FLOAT` | :87 |
| `Ty::Primitive(String)` | `Ty::String` | `name string` | `java.lang.String` | `java.lang.String` | `BamlType.STRING` | :88 |
| `Ty::Primitive(Bool)` | `Ty::Bool` | `active bool` | `boolean` | `java.lang.Boolean` | `BamlType.BOOL` | :89 |
| `Ty::Primitive(Null)` | `Ty::Null` | `null` in a union | `java.lang.Void` | `java.lang.Void` | `null` (wire-driven) | :91 |
| `Ty::Primitive(Uint8Array)` | `Ty::Uint8Array` | `data uint8array` | `byte[]` | `byte[]` | `null` (wire-driven) | :100 |
| `Ty::Primitive(Image)` | `Ty::Media(Image)` | `photo image` | `baml_sdk.baml.media.Image` | (same) | `null` (wire-driven) | :102 |
| `Ty::Primitive(Audio)` | `Ty::Media(Audio)` | `clip audio` | `baml_sdk.baml.media.Audio` | (same) | `null` (wire-driven) | :103 |
| `Ty::Primitive(Video)` | `Ty::Media(Video)` | `clip video` | `baml_sdk.baml.media.Video` | (same) | `null` (wire-driven) | :104 |
| `Ty::Primitive(Pdf)` | `Ty::Media(Pdf)` | `doc pdf` | `baml_sdk.baml.media.Pdf` | (same) | `null` (wire-driven) | :105 |
| generic media source type | `Ty::Media(Generic)` | `media` / any media shape | `java.lang.Object` | (same) | `null` (wire-driven) | :107 |
| `Ty::Literal(Int(v), …)` | `Ty::Literal(Int(v))` | `answer 42` | `long` | `java.lang.Long` | `BamlType.literalInt(42L)` | :94 |
| `Ty::Literal(Bigint(v), …)` | `Ty::Literal(Bigint(v))` | `answer 42n` | `java.math.BigInteger` | `java.math.BigInteger` | `BamlType.literalBigint(new java.math.BigInteger("42"))` | :95 |
| `Ty::Literal(Float(_), …)` | `Ty::Literal(Float(_))` | float literal type | `double` | `java.lang.Double` | `BamlType.literalFloat("<v>")` | :96 |
| `Ty::Literal(String(v), …)` | `Ty::Literal(String(v))` | `status "draft"` | `java.lang.String` | `java.lang.String` | `BamlType.literalString("draft")` | :97 |
| `Ty::Literal(Bool(v), …)` | `Ty::Literal(Bool(v))` | `flag true` | `boolean` | `java.lang.Boolean` | `BamlType.literalBool(true)` | :98 |
| `Ty::EnumVariant(qtn, variant, …)` | `Ty::Enum(qtn)` | specific enum variant type | enum FQN `baml_sdk.<ns>.<Enum>` | (same) | `BamlType.classByFqn("user.<ns>.<Enum>")` | :125 |
| `Ty::Class(qtn, args, …)` | `Ty::Class(name, args)` | `resume Resume` | `baml_sdk.lorem.Resume`, `baml_sdk.generics.Wrapper<java.lang.Long>` | (same; args always boxed) | `BamlType.classByFqn("<fqn>")` (args dropped) | :109–121 |
| `Ty::Enum(qtn, …)` | `Ty::Enum(name)` | `sentiment Sentiment` | `baml_sdk.ipsum.Sentiment` (generated Java `enum`) | (same) | `BamlType.classByFqn("<fqn>")` | :125 |
| `Ty::TypeAlias(qtn, …)` | `Ty::TypeAlias(name)` | `items StringList` | erases to resolved type; **recursive** → nominal FQN `baml_sdk.<ns>.<Alias>` | (same) | resolved's descriptor; **recursive** → `BamlType.classByFqn("<fqn>")` | :126–134 |
| `Ty::TypeVar(name, …)` | `Ty::TypeVar(name)` | generic type parameter `T` | bare `T` (java identifier) | (same) | `BamlType.typeVar("T")` | :135 |
| `Ty::Optional(T, …)` | `Ty::Union([T, Null])` | `name string?` | boxed inner `T` (e.g. `int?` → `java.lang.Long`) | (same) | inner descriptor (`T?` collapses to inner) | :199–208 |
| `Ty::List(T, …)` | `Ty::List(T)` | `tags string[]` | `java.util.List<T>` (T boxed) | (same) | `BamlType.list(D)` | :136–139 |
| `Ty::Map(K, V, …)` | `Ty::Map { key, value }` | `metadata map<string,int>` | `java.util.Map<java.lang.String, V>` (key forced to String) | (same) | `BamlType.map(BamlType.STRING, Dval)` | :142–145 |
| `Ty::Union(types, …)` | `Ty::Union(types)` | `result string \| int` | `baml_bridge.Union2<…>` … `Union10<…>`; arity>10 → `java.lang.Object`; same-base literal union → base | (same) | `BamlType.union(a, b, …)` ordered | :146, :198–233 |
| `Ty::Unknown { … }` | `Ty::Unknown` | `unknown` keyword | `java.lang.Object` | (same) | `null` (wire-driven) | :147 |
| `Ty::Function { params, ret, throws, … }` | `Ty::Function { params, ret }` | callable type | `java.util.function.*` by arity; optional/arity>2 → generated `@FunctionalInterface` (`IntOptCallback` shape, landed `202883518`) | (same) | `null` (wire-driven) | :148, :407–435 |
| `Ty::Void { … }` | `Ty::Void` (Python calls it `Ty::Unit`) | `-> void` | `void` | `java.lang.Void` | `null` (wire-driven) | :149–152 |
| no direct TIR variant | `Ty::BamlOptions` (Python-only) | generated function options plumbing | — no CodegenTy variant; options ride the trailing configurator overload | — | — | n/a |
| `Ty::RustType { … }` | `Ty::RustType` | opaque builtin state | `baml_bridge.BamlHandle` | (same) | `null` (wire-driven) | :153 |
| `Ty::Type { … }` | `Ty::Type` | `type` metatype keyword | `java.lang.Object` | (same) | `null` (wire-driven) | :157–162 |
| `Ty::Never { … }` | `Ty::Never` | divergent expr / `throws never` | `java.lang.Object` | (same) | `null` (wire-driven) | :157–162 |
| `Ty::Future(value, error, …)` | `Ty::Future` | `spawn { … }` before `await` | `java.lang.Object` | (same) | `null` (wire-driven) | :157–162 |
| (no TIR row in Python) | `Ty::Interface` | interface type | `java.lang.Object` | (same) | `null` (wire-driven) | :157–162 |
| (no TIR row in Python) | `Ty::Resource` | resource type | `java.lang.Object` | (same) | `null` (wire-driven) | :157–162 |
| (no TIR row in Python) | `Ty::PromptAst` | prompt-AST type | `java.lang.Object` | (same) | `null` (wire-driven) | :157–162 |
| `Ty::Error { … }` | no CodegenTy variant | hard error sentinel | never reaches codegen | — | — | n/a |

Per-row deviation flags:

> ⚠ **Deviation from Python (nullability model — pervasive):** `T?` renders as the **boxed
> nullable** inner type, **never `java.util.Optional<T>`** and never a union type
> (translate_union :199–208, `primitive` boxing :166–171). Python uses `typing.Optional[T]`.
> Reason: Java has no zero-cost `Optional`, and boxing already encodes nullability at the type
> level (`long` non-null vs `java.lang.Long` nullable) — a dedicated `@Nullable` annotation
> dependency was deferred (translate_ty.rs:11–16). The null arm is stripped both in the 1-arm
> collapse and inside multi-arm unions.

> ⚠ **Deviation from Python (literals):** literal types **erase to their base Java type** — Java
> has no literal types. `"draft" | "sent"` → `java.lang.String`, `flag true` → `boolean`
> (translate_ty.rs:93–99, `common_literal_base` :239–259). Python emits `typing.Literal[…]`. As a
> side benefit Java has no float-literal hole: a `Ty::Literal(Float)` becomes `double`, whereas
> Python must fall back to `typing.Any` because `typing.Literal` rejects floats. The **descriptor**
> for a standalone literal still preserves the value (`BamlType.literalInt(42L)`,
> `BamlType.literalString("draft")`); a *same-base literal union* descriptor collapses to the base
> primitive (`BamlType.STRING` / `.INT` / …, `descriptor_union_expr`).

> ⚠ **Deviation from Python (map keys):** the Java map type is **always
> `java.util.Map<java.lang.String, V>`** — every BAML map key (str/int/bool/enum) is stringified
> engine-side, so the key slot is hard-coded to `String` (translate_ty.rs:140–145). Python emits
> `typing.Dict[K, V]` preserving `K`. Note the **descriptor** does preserve the declared key token
> (`BamlType.map(BamlType.STRING, …)`), since `descriptor_expr` recurses on the real key (in
> practice `String`) — a map union arm matches on it.

> ⚠ **Deviation from Python (unknown / unmodeled types):** `Ty::Unknown`, and the
> not-yet-modeled `Ty::Type` / `Ty::Never` / `Ty::Future` / `Ty::Interface` / `Ty::Resource` /
> `Ty::PromptAst`, all fall back to `java.lang.Object` (translate_ty.rs:147, :157–162; descriptor
> `unknown`, lib.rs:401–407). Python drops `Type`/`Never`/`Future` as **unreachable / n/a** for a
> Python type; Java gives them an explicit `Object` fallback so surrounding generated code still
> compiles (the same stance as Python's `typing.Any` / TS's `unknown`). `Ty::Error` has
> **no CodegenTy variant at all** in Java, matching Python's "never reaches codegen".

> ⚠ **Deviation from Python (options plumbing):** Python has a codegen `Ty::BamlOptions` →
> `baml.Options`. Java has no such Ty; per-call options ride the AWS-SDK-v2-style **trailing
> configurator overload** (`Fns.probe(1, o -> o.opt1(5))`) and the trailing `BamlCallContext`,
> not a value type (ref-java-codegen-conventions.md:78–83).

### Unions — the biggest deviation from Python

> ⚠ **Deviation from Python (the whole union family):** Python renders `typing.Union[...]` — a
> structural, duck-typed set; the decoder discards union metadata and unwraps to the inner value.
> Java renders the **runtime generic arity family** and reconstructs a **typed arm record** on
> decode. TEAM DECISION 2026-07-16 (translate_ty.rs:216–230, conventions doc:50–64):

- **Anonymous multi-arm unions** (2..10 non-null arms) render inline as
  `baml_bridge.Union{n}<Arm0, …, Arm{n-1}>` — sealed interfaces with nested generic records
  `Arm0..Arm{n-1}`, **one per positional arm in BAML declaration order** (post-normalization, null
  arm stripped), arms boxed (translate_ty.rs:226–230). Java 21+ consumers get exhaustive `switch`
  with record patterns; Java 17 uses `instanceof`. Verified:
  `baml_bridge.Union2<baml_sdk.baml.csv.CsvRecord, baml_sdk.baml.iter.Done>`.
- **Arm selection is type-directed at decode time**, matching the wire value against the *declared*
  arm list in source order — the wire's `value_option_name` / arm order is never trusted. So **no
  nominal type is minted** for anonymous unions and the `UnionSink` stays empty (it is vestigial,
  retained only because `translate_ty`'s signatures still thread `&mut UnionSink`;
  translate_ty.rs:54–68).
- **Same-base literal unions erase to the base type** (`"draft" | "sent"` → `java.lang.String`),
  descriptor collapses to the base primitive (`descriptor_union_expr`).
- **`T | null` collapses to boxed `T`** — nullability by boxing, never a union type
  (translate_ty.rs:204–208).
- **Arity > 10 falls back to `java.lang.Object`** — the `Union2..Union10` family is exhausted.
  **NOT YET IMPLEMENTED IN JAVA** as a named type: the threshold / alias policy is **OPEN**; until
  it lands the decoder uses the wire-driven path (translate_ty.rs:222–225).
- **Recursive type aliases are the exception:** they keep a **minted nominal sealed type named
  after the alias** (a positional generic cannot reference itself), rendered by the emitter's
  `render_union` (translate_ty.rs:18–31, 126–134). Verified `json` sealed interface:
  ```java
  public sealed interface json permits json.BooleanValue, json.IntValue, json.FloatValue,
      json.StringValue, json.jsonListValue, json.jsonMapValue {
      record BooleanValue(java.lang.Boolean value) implements json {}
      record IntValue(java.lang.Long value) implements json {}
      record jsonListValue(java.util.List<baml_sdk.baml.json.json> value) implements json {}
      record jsonMapValue(java.util.Map<java.lang.String, baml_sdk.baml.json.json> value) implements json {}
      // …
  }
  ```

**Descriptor for unions** (`descriptor_expr` / `descriptor_union_expr`, translate_ty.rs):
- ordered anonymous union → `BamlType.union(a, b, …)` (declaration order — the decoder matches
  arms structurally in this order).
- `T | null` → the inner descriptor (nullability never affects decode).
- same-base literal union → the erased base primitive (`BamlType.STRING`, `.INT`, …).
- a union carrying an unresolved type-var arm → `null` (wire-driven `self_type` decode).

**Registry side of a union** (`registerUnion` / `registry_arm_expr`, lib.rs / translate_ty.rs): a
union's registry key is its **arm SET** — a sorted, distinct `List<BamlType>` (null arms excluded;
`List.equals` over `BamlType` value equality is order- and duplicate-insensitive, so engine-side
normalization can't cause a mismatch — no rendered string, so no crafted-literal collision). The
runtime derives the equal arm set from the wire `self_type` (`ProtoReader.wireArmType`); a recursive
alias registers a second time under its FQN (`registerUnionAlias`). Arm tokens are typed
`BamlType`s (`registry_arm_expr`). Arm record binary names come from `union_arm_token` (translate_ty.rs):
class/enum/alias arms → the identifier + `Value`; container arms → `{token}ListValue` / `{token}MapValue`;
**literal arms use a `K`-prefixed token** (legacy Go precedent) — `"draft"` → `Kdraft`, `1` → `IntK1`,
`true` → `BoolKTrue`. Verified `registerUnion("baml_sdk.baml.json.json", new baml_bridge.BamlType[]
{BamlType.BOOL, BamlType.INT, BamlType.FLOAT, BamlType.STRING, BamlType.list(BamlType.classByFqn(
"baml.json.json")), BamlType.map(BamlType.STRING, BamlType.classByFqn("baml.json.json"))}, {
"…json$BooleanValue", "…json$IntValue", …})` plus a `registerUnionAlias("baml.json.json", …)`.

### Callables — `java.util.function` by arity

`translate_callable` (translate_ty.rs:407–435) maps a `(params) -> ret` onto a
`java.util.function` shape by arity and whether the return is unit:

| arity | returns value | returns `void` |
| --- | --- | --- |
| 0 | `java.util.function.Supplier<R>` | `java.lang.Runnable` |
| 1 | `java.util.function.Function<P0, R>` | `java.util.function.Consumer<P0>` |
| 2 | `java.util.function.BiFunction<P0, P1, R>` | `java.util.function.BiConsumer<P0, P1>` |

Params and return are boxed. Verified `Function<java.lang.Long, java.lang.String>` and
`BiFunction<…>` in generated `Fns.java` and `Array.generate(…, Function<java.lang.Long, T> f)`.

> ⚠ **Deviation from Python (callables):** Python renders `typing.Callable[[...], ret]` and widens
> to `typing.Callable[..., ret]` when **any** parameter is optional. Java has no variadic callable
> type, so it maps by concrete arity to `java.util.function.*`.

> **LANDED (`202883518`, decision E1):** a callable **with any optional parameter or arity > 2**
> — which has no `java.util.function.*` equivalent — is emitted as a generated
> `@FunctionalInterface extends baml_bridge.BamlHostCallable` with a fixed-arity SAM plus a nested,
> always-constructed `Opts` bag with **nullable** accessors, and a `default __bamlDispatch(...)` that
> reshapes the bridge's flat declared-order arg list into the SAM. Verified `IntOptCallback` with
> `Long apply(Long x, Opts $opts)` and `Opts(Long y, Long z)` (nullable). One interface per distinct
> signature within the namespace, signature-derived name; the documented non-null-bag divergence
> stands. The descriptor for a callable param is `null` (wire-driven).

Notes:
- Generated classes are **immutable `public final` value classes**: a canonical all-args constructor
  in field declaration order, PreserveCase accessor methods (`p.int_field()`), and deep value equality —
  `equals` / `hashCode` handle `byte[]` fields via `Arrays.equals`. `final` is load-bearing: the encoder
  keys its typemap on the exact runtime class, so a user subclass would silently break inbound-encode.
  All generated value classes — plain, generic, handle-backed, and `$stream` companions — are final
  (sealed union interfaces and their already-final permitted records are the exception). **POJOs, not
  `record`s** (decided 2026-07-17): deep `byte[]` equality has to be hand-generated regardless, and the
  reified type-args ride a weak-identity side-table rather than a hidden instance field
  (conventions doc:42–49).
- Class fields are plain `(name, type)` pairs. An optional field renders as the **boxed nullable
  type** (`java.lang.Long field`), never `Optional<T>` and never with a default — required-but-nullable,
  mirroring Pydantic's stance. `registerClass` carries a parallel `String[] fieldDescs` of descriptor
  tokens for type-directed decode.
- **Recursion needs no forward-ref machinery** — Java FQNs resolve at class-load, so recursive
  classes reference each other's FQN directly (no `from __future__ import annotations` analog).
  Recursive **type aliases** mint a nominal sealed type named after the alias (erasure would not
  terminate) — the `json` example above. Python instead uses `typing_extensions.TypeAliasType(...)`.
- Same features dropped as Python: no `Checked<T>` / `@check` / `@@check`, no `StreamState<T>` /
  `@stream.state`, no `@@dynamic` class / enum codegen.

## Java-specific codegen notes

- **One `.java` file per generated type** — Java has a single compiled artifact carrying its own type
  information, so there is no runtime/stub split (Python's `.py` + `.pyi`). Because every emitted type
  expression is a fully-qualified name, the import-collection machinery the TS emitter needs
  (`TranslatedType { expr, imports }`) collapses to a plain `String` (translate_ty.rs:1–7).
- **Runtime init via a root-holder static initializer.** Loading any generated class triggers
  idempotent runtime initialization from **embedded bytecode** (base64/MIME-embedded, decoded with
  `java.util.Base64.getMimeDecoder()`), the Java analog of Python's root-package import side effect
  (conventions doc:84–87). There is no `_inlinedbaml.py` / `_typemap.py` / `py.typed` triplet; the
  BAML source and the FQN⇄class map live in `TypeRegistry` registrations run at init.
- **Free functions → static methods on a per-namespace `Fns` holder** (`baml_sdk.<ns>.Fns.<fn>()`;
  root-namespace on `baml_sdk.Fns`; escapes to `Fns$` on collision). Every callable gets a `_async`
  sibling returning `java.util.concurrent.CompletableFuture<T>`, plus trailing `BamlCallContext`
  overloads. `$`-companions keep the BAML name verbatim — no `__` / `_stream` mangling.
- **`TypeRegistry.registerClass / registerEnum / registerUnion`** at init map BAML FQN ⇄ generated
  Java class and carry the per-field / per-arm **typed `baml_bridge.BamlType` descriptors** (the
  type-directed decode side-table; a union is keyed structurally by its arm set). Each generated
  binding passes its return-type descriptor as the **last positional arg** of
  `BamlFfi.callSync/callAsync` — a per-holder `private static final BamlType $RET{n}` constant, or
  the literal `null` for a wire-driven return.
- **Descriptor** (a typed `baml_bridge.BamlType` data structure — `descriptor_expr`, translate_ty.rs):
  primitives `BamlType.INT | FLOAT | STRING | BOOL`; class/enum/named-recursive-alias →
  `BamlType.classByFqn("<baml fqn>")` (registry resolves the kind); `BamlType.list(D)`;
  `BamlType.map(BamlType.STRING, D)`; ordered union `BamlType.union(D, D, …)`; a literal
  `BamlType.literalString("…")` / `literalInt(…L)` / …; a type var `BamlType.typeVar("<name>")`.
  Everything wire-driven (bigint / uint8array / null / void / media / callable / handle / the
  `unknown`-family) passes the literal `null`. Decode without a descriptor, or on any
  descriptor/value mismatch that has a self-describing wire form, falls back to the wire-driven path
  (error/panic values are always wire-driven).

## Java error handling

Function calls return a `BamlOutboundResult` envelope; `ProtoReader.decodeOutboundResult` dispatches its
`ok` / `error` / `panic` arm (ProtoReader.java:199–217).

- **`ok`** decodes and returns the value, type-directed by the return descriptor
  (`decodeWithDesc(okBytes, …, /*lenient*/ false)`).
- **`error`** decodes the payload and throws an **unchecked `baml_bridge.BamlError`** carrying
  `.value()`, `.baml_trace()`, and `.class_name()`. Special case: an `error` whose value FQN is
  `baml.errors.TypeMismatch` is re-surfaced as a native `java.lang.IllegalArgumentException` (message
  from the decoded value; BAML frames synthesized into real `StackTraceElement`s and spliced onto the
  exception). **LANDED (`74782a679`, D2).**
  > ⚠ **Deviation from Python:** Python remaps `TypeMismatch` to a native `TypeError`; Java has no
  > `TypeError`, so it maps to `IllegalArgumentException` — the idiomatic "bad argument" exception
  > and the intended 1:1 analog (java-function-calls-decisions.md, D2).
- **`error` carrying a rehydratable host-callable exception** (`baml.errors.HostCallable`) →
  re-throw the **original** native Java `Throwable` by identity, looked up in the Java-side host-value
  registry (`BamlFfi.lookupHostValue` → `sneakyThrow`, so `assertSame` holds). **LANDED (`202883518`):**
  registry design A1 + identity rehydration (D) + the 1-arg `BamlError(Object value)` ctor (F) are all
  built; a foreign/released key falls through to a metadata `BamlError`. See
  ref-java-outbound-decoding.md, error arm.
- **`panic`** (non-exit) → throw an unchecked `baml_bridge.BamlPanic(value, bamlTrace, className)`.
  **`BamlPanic` re-parents to `java.lang.Error`** (`BamlPanic.java:20`) so it escapes
  `catch (Exception)` — the analog of Python raising `BamlPanic` off `BaseException`. **LANDED
  (`74782a679`).**
- **Exit panics** (`baml.sys.exit`, `is_exit_panic`) → run the registered telemetry-flush hooks
  (`BamlFfi.runExitFlushHooks()`, best-effort) then `Runtime.getRuntime().halt(exitCode)` (hard exit,
  bypasses shutdown hooks — analog of Python `os._exit`). **Flush hook wired, LANDED (`eab6d37cc`);**
  no telemetry ships in this slice, so the hook registry is empty by default. `ProtoReader.decodePanic`
  decodes `is_exit` / `exit_code` and drives the flush-then-halt (`ProtoReader.java:313-319`).
- **Cancellation** — **DECIDED 2026-07-17 (D1), full Python parity plus a sync path.** Trailing
  `BamlCallContext` overloads (`f(req…, ctx)`, `f(req…, opts, ctx)`); engine-driven abort →
  `BamlCancelledError extends java.util.concurrent.CancellationException` (the future counts as
  cancelled; `join()` / `get()` throw it unwrapped); `future.cancel(true)` →
  `nativeCancelFunctionCall(callId)` + a raw `CancellationException`; **sync** abort → `BamlPanic(Cancelled)`.
  > ⚠ **Deviation from Python:** Python wraps the engine `baml.panics.Cancelled` into
  > `asyncio.CancelledError` and has **no sync cancellation path**. Java exceeds parity with a sync
  > path (`BamlPanic(Cancelled)`) and uses `CancellationException` rather than a coroutine-cancel signal.
- **Documented throws:** Java does not encode BAML `throws` as checked exceptions; thrown BAML type
  names are emitted as **Javadoc `@throws`** tags — the analog of Python's `Raises:` docstrings
  (TestRaises 8/8; state-of-completeness doc:40).

## BexExternalValue conversions

The runtime value type returned by `bex_engine`, serialized to a `BamlOutboundValue` protobuf and
decoded **Java-side** by `ProtoReader.decodeValue` / `decodeOutboundResult`. `ok` values are decoded
strictly (`lenient=false`); `error` / `panic` payloads are decoded leniently (`lenient=true`,
ProtoReader.java:238, :322) — the lenient path returns `null` where the strict path would throw
`unsupported`. "(handle table)" means the wire payload is a handle key + `BamlHandleType`
discriminator resolved from the per-call handle table.

| `BexExternalValue` variant | `BamlOutboundValue.value` oneof field | Decoded Java value |
| --- | --- | --- |
| `Null` | `null_value` (`BamlValueNull`) | `null` (ProtoReader :344) |
| `Int(i64)` | `int_value` (`int64`) | `java.lang.Long` (:349) |
| `Bigint(BigInt)` | `bigint_value` (`string`, base-16) | `java.math.BigInteger` — `new BigInteger(str, 16)` (:357) |
| `Float(f64)` | `float_value` (`double`) | `java.lang.Double` (:350) |
| `Bool(bool)` | `bool_value` (`bool`) | `java.lang.Boolean` (:351) |
| `String(String)` | `string_value` (`string`) | `java.lang.String` (:348) |
| `Uint8Array(Vec<u8>)` | `uint8array_value` (`bytes`) | `byte[]` (:356) |
| — | `literal_value` (`BamlLiteralValue`) | `decodeLiteral` unwraps to the inner `Long` / `String` / `Boolean` / `BigInteger` / `Double`; envelope discarded. No BEX variant produces this on the FFI path (:352, :376–392) |
| `Array { element_type, items }` | `list_value` (`BamlValueList`) | `java.util.List` (`ArrayList`), elements recursively decoded (:353, :394) |
| `Map { key_type, value_type, entries }` | `map_value` (`BamlValueMap`) | `java.util.Map` (`LinkedHashMap`), **String keys**, values recursively decoded (:354, :409) |
| `Instance { class_name, fields, type_args }` | `class_value` (`BamlValueClass`) | generated class in `baml_sdk/` (or in-package `<Name>$stream` companion), resolved via `TypeRegistry`; `type_args` reify generics (:358) |
| `Variant { enum_name, variant_name }` | `enum_value` (`BamlValueEnum`) | generated Java `enum`, resolved via `TypeRegistry` (:359) |
| `Union { value, metadata }` | `union_variant_value` (`BamlValueUnionVariant`) | **type-directed reconstruction**: `self_type` tokenized to the sorted `\|`-signature, arm picked structurally from the inner value, `TypeRegistry.constructUnion` returns the generated arm record (`Union{n}` arm or recursive-alias sealed record); unregistered signature / unmatched arm → bare decoded inner value; `value_option_name` never trusted (:355, :452–484) |
| `Handle(Handle)` | `handle_value` (`BamlOutboundHandle` — handle table) | media handle types → typed stdlib class wrapping `BamlHandle` (`Image` / `Audio` / `Video` / `Pdf`, dispatched on `ADT_MEDIA_*`); every other handle type → bare `baml_bridge.BamlHandle` (:360, :109–110) |
| `FunctionRef { global_index }` | `handle_value` (`FUNCTION_REF`) | bare `baml_bridge.BamlHandle` — no wrapper, not callable back (parity with Python) |
| `Adt(Collector(CollectorRef))` | `handle_value` (`ADT_COLLECTOR`) | bare `baml_bridge.BamlHandle` |
| `Adt(Type(RuntimeTy))` | `ty_value` (`BamlTy`) | lenient path → `null` (`OV_TY` skipped, :361–368); strict path → throws `unsupported`. BAML type-reference values do not round-trip. |
| `Adt(PromptAst(Arc<PromptAst>))` | handle table (`ADT_PROMPT_AST`) on the FFI path; inline `prompt_ast_value` never used | bare `baml_bridge.BamlHandle`; an inline `prompt_ast_value` (`OV_PROMPT_AST`) → `null` (lenient) / throws `unsupported` (strict) (:361–368) |
| `Adt(Media(Arc<MediaValue>))` | handle table (`ADT_MEDIA_*`) on the FFI path; inline `media_value` never used | typed media stdlib class via the `Handle(...)` row; inline `media_value` (`OV_MEDIA`) → `null` (lenient) / throws `unsupported` (strict) (:361–368) |
| `Adt(TaggedHeapHandle { ty, .. })` | `handle_value` with `handle_type = ADT_TAGGED_HEAP_HANDLE` (14); `ty` carries the erased `TPartial`/`TFinal` | `baml_bridge.BamlStream<TPartial,TFinal>` via `BamlStream.fromHandle` — the `handle_type` tag alone picks it (Java does **not** read `ty`). **`BamlStream` LANDED (`a6e3ca99e`)**, llm_functions 21/21. (`$rust_type` shells decode via the `class_value` path, not here.) |
| `RustData(Arc<dyn Any>)` | `try_convert_rust_data`, else `handle_value` (`UNTAGGED_RUST_DATA`) | converted → recurse; otherwise bare `baml_bridge.BamlHandle` stored in the shell class's private `_handle` field |
| `HostValue(HostValueArc)` | `handle_value` with `HOST_VALUE_CALLABLE` (15) / `HOST_VALUE_OPAQUE` (16) | **LANDED (`202883518`):** on the error arm a `HOST_VALUE_OPAQUE` throwable is rehydrated **by identity** from the Java-side registry (`lookupHostValue` → original `Throwable` via `sneakyThrow`); a foreign/released key falls through to a metadata `BamlError`. |
| — | `media_value` (`BamlValueMedia`) inline | lenient → `null`; strict → throws `unsupported` — media always rides `handle_value` on the FFI path (:361–368) |
| — | `prompt_ast_value` (`BamlValuePromptAst`) inline | lenient → `null`; strict → throws `unsupported` — same rationale (:361–368) |

> ⚠ **Deviation from Python (union decode):** Python discards union metadata and unwraps to the
> inner value (the language is duck-typed). Java **reconstructs the typed arm record** via the
> descriptor + `TypeRegistry` (the Go-like path) — this is the decode-side face of the union
> deviation above.

> ⚠ **Deviation from Python (`$stream` instance routing):** an `Instance` for a `$stream` companion
> decodes into the **in-package `<Name>$stream`** class, not Python's `baml_sdk.stream_types.*`
> (decided 2026-07-17: in-package `$stream` companions stay).

> ⚠ **Deviation from Python (inline media / prompt_ast / ty):** where Python **raises `BamlError`**
> if it ever sees an inline `media_value` / `prompt_ast_value` (and returns terminal `None` for
> `ty_value`), Java's decoder **skips to `null` on the lenient path** and throws a generic
> `unsupported` on the strict path. Same net "shouldn't happen on the FFI path" intent, different
> mechanism.
