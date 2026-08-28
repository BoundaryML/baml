---
date: 2026-07-20
repository: baml4
mirrors: sdks/agent-docs/bridge-ref/ref-python-examples.md
source_fixtures:
  - baml_language/sdk_tests/fixtures/type_shapes/baml_src
  - baml_language/sdk_tests/crates/java/type_shapes/generated/baml_sdk
  - baml_language/sdk_tests/fixtures/function_calls/baml_src
  - baml_language/sdk_tests/crates/java/function_calls/generated/baml_sdk
emitter:
  - baml_language/sdks/java/sdkgen_java/src/lib.rs
  - baml_language/sdks/java/sdkgen_java/src/emit.rs
  - baml_language/sdks/java/sdkgen_java/src/routing.rs
  - baml_language/sdks/java/sdkgen_java/src/translate_ty.rs
runtime:
  - baml_language/sdks/java/baml_bridge/src/main/java/baml_bridge
---

# Java Codegen Examples From SDK Tests

This file records the Java output that the generated SDK tests exercise today.
It is written as a **section-for-section mirror of `ref-python-examples.md`**:
the same headings, the same order, the same fixtures — so the two can be read
side by side to review every place the Java surface diverges from the Python
prior art. Each section shows the BAML source (where the Python doc shows it),
then the **real generated Java** pulled from
`sdk_tests/crates/java/{type_shapes,function_calls}/generated/baml_sdk/**`
(regenerate with `bash sdk_tests/crates/java/setup.sh`), then call-site usage.

**Flag conventions**

- `> ⚠ **Deviation from Python:** …` — a place where the Java surface is
  deliberately shaped differently from Python, with the one-sentence reason and
  an emitter/runtime `file:line` citation so the review can jump to the code.
- `**NOT YET IMPLEMENTED IN JAVA**` — a capability the Python doc documents that
  Java has not built yet. The line records the **decided design** (from
  `thoughts/antonio/java-function-calls-decisions.md`) or the **open decision**.

Every claim below cites code that was read or output that is quoted; no behavior
is inferred. Snippets are re-quoted from the current generated trees; line
numbers are indicative (last audited 2026-07-20, after the typed-descriptor,
host-callable, and streaming landings).

**One structural deviation applies to the entire document**, so it is stated
once here rather than repeated per section:

> ⚠ **Deviation from Python:** There is no `.py` / `.pyi` split. Python emits a
> runtime `__init__.py` (Pydantic models + `define_function` bindings) plus a
> typed `__init__.pyi` stub. Java emits **one `.java` file per top-level type**
> (the JLS allows one public type per file), and that single file is *both* the
> runtime binding and the typed surface — the static Java type system needs no
> separate stub. Reason: Java is statically typed and compiles the bindings
> directly. `sdkgen_java/src/lib.rs:6-18`.

## File Layout

Java routes each BAML namespace to a package **directory** under `baml_sdk/`,
one `.java` file per generated type, and one `Fns.java` free-function holder per
package that declares free functions.

Type-shape fixture paths (representative):

```text
baml_sdk/Baml.java                          # runtime anchor (typemap + bytecode init)
baml_sdk/Fns.java                           # root-namespace free functions
baml_sdk/Foo.java
baml_sdk/Foo$stream.java                    # PPIR stream partial-output model, in-package
baml_sdk/inlinedbaml.b64                    # compiled bytecode, base64 resource
baml_sdk/primitives/Fns.java
baml_sdk/primitives/Primitives.java
baml_sdk/primitives/Primitives$stream.java
baml_sdk/enums/Sentiment.java
baml_sdk/symbol_collisions/lorem/Ipsum.java
baml_sdk/void$/Fns.java                      # `void` namespace, keyword-escaped
```

Function-call fixture paths follow the same pattern
(`baml_sdk/methods_on_classes/Greeter.java`, `baml_sdk/raises_test/Fns.java`,
`baml_sdk/host_callable_tests/Fns.java`, …).

> ⚠ **Deviation from Python:** No barrel / `__init__` files and no lazy child
> re-export machinery. Python emits an `__init__.py` per package with a PEP 562
> `__getattr__` lazy-loader over a `_LAZY_CHILDREN` frozenset; Java packages
> exist by virtue of their directory, so a container namespace emits *nothing*.
> Reason: Java has no module-object indirection to lazily populate.
> `sdkgen_java/src/lib.rs:15-16`.

> ⚠ **Deviation from Python:** PPIR `$stream` partial-output models stay **in the base type's
> package** with a `$stream`-suffixed name (`Foo$stream`, `primitives/Primitives$stream`),
> because `$` is a legal Java identifier character. Python cannot use `$` in an
> identifier, so it emits a **parallel `stream_types/<ns>` package** and routes
> the `$stream` FQN there via the type map. Reason: keep the BAML name verbatim;
> no parallel package tree. `sdkgen_java/src/routing.rs:19-23`. (See
> *Stream Partial-Output Types*.)

> ⚠ **Deviation from Python:** Package/segment names that collide with Java
> reserved words are escaped with a trailing `$` (`void` → `void$`), not left
> as-is. Python only has to avoid `$` (not a Python identifier char); Java has
> to avoid the JLS keyword set. `sdkgen_java/src/routing.rs:56-133` (keyword
> table + `java_identifier`). Real output: `baml_sdk/void$/Fns.java` declares
> `package baml_sdk.void$;`.

## Root Package

Derived from `type_shapes/generated/baml_sdk/Baml.java` and `Fns.java`.

The root anchor `Baml.java` is the Java analog of Python's root-package import
side effect: loading it registers the type map and initializes the runtime from
the embedded bytecode. Every `Fns` holder forces it via a `static { Baml.ensure(); }`
block.

```java
// baml_sdk/Baml.java (trimmed — one registration + the bytecode init)
public final class Baml {
    private Baml() {}

    static {
        baml_bridge.TypeRegistry.registerClass("user.primitives.Primitives", "baml_sdk.primitives.Primitives", new java.lang.String[] {"int_field", "float_field", "string_field", "bool_field", "null_field", "uint8array_field"}, new baml_bridge.BamlType[] {baml_bridge.BamlType.INT, baml_bridge.BamlType.FLOAT, baml_bridge.BamlType.STRING, baml_bridge.BamlType.BOOL, null, null});
        // … registerClass / registerEnum / registerUnion / registerUnionAlias for every symbol …
        try (java.io.InputStream in = Baml.class.getResourceAsStream("/baml_sdk/inlinedbaml.b64")) {
            if (in == null) {
                throw new IllegalStateException("baml_sdk/inlinedbaml.b64 not found on the classpath — …");
            }
            byte[] b64 = in.readAllBytes();
            byte[] bytecode = java.util.Base64.getMimeDecoder().decode(b64);
            baml_bridge.BamlFfi.initFromBytecode(bytecode);
        } catch (java.io.IOException e) {
            throw new java.io.UncheckedIOException("failed to read embedded BAML bytecode", e);
        }
    }

    /** Forces class initialization (and thus runtime init). No-op afterwards. */
    public static void ensure() {}
}
```

Root-namespace user symbols are direct package members: `Foo.java` is the value
class, `Fns.java` holds the free functions.

```java
// baml_sdk/Fns.java
public final class Fns {
    private Fns() {}

    static {
        baml_sdk.Baml.ensure();
    }
    // Pooled, per-holder decode descriptor: a typed BamlType data structure
    // (deduped across the holder's bindings), NOT a string.
    private static final baml_bridge.BamlType $RET0 = baml_bridge.BamlType.classByFqn("user.Foo");

    public static baml_sdk.Foo make_foo(long v) {
        return (baml_sdk.Foo) baml_bridge.BamlFfi.callSync("user.make_foo", new java.lang.String[] {"v"}, new java.lang.Object[] {v}, $RET0);
    }

    @SuppressWarnings("unchecked")
    public static java.util.concurrent.CompletableFuture<baml_sdk.Foo> make_foo_async(long v) {
        return (java.util.concurrent.CompletableFuture<baml_sdk.Foo>) (java.util.concurrent.CompletableFuture<?>) baml_bridge.BamlFfi.callAsync("user.make_foo", new java.lang.String[] {"v"}, new java.lang.Object[] {v}, $RET0);
    }

    // + trailing-`ctx` overloads make_foo(v, ctx) / make_foo_async(v, ctx) (pass $RET0, ctx);
    //   round_trip_foo(...) similarly, sharing the same pooled $RET0 constant
}
```

Call sites reach a namespace through its package's `Fns` holder:

```java
import baml_sdk.Fns;

baml_sdk.Foo foo = baml_sdk.Fns.make_foo(1);
baml_sdk.Foo fooA = baml_sdk.Fns.make_foo_async(1).join();          // async = future, sync = join
long n = baml_sdk.primitives.Fns.return_int();
baml_sdk.symbol_collisions.lorem.Ipsum ip =
    baml_sdk.symbol_collisions.lorem.Fns.make_ipsum(bar1, bar2, bar3);
```

> ⚠ **Deviation from Python:** There are **no module-level free functions and no
> `b` client object**. Python exposes `b.make_foo_async(1)` and
> `b.primitives.return_int_async()` — the package *is* the client, functions are
> package attributes. Java has no free functions, so each namespace's functions
> are `static` methods on a per-package `Fns` holder
> (`baml_sdk.primitives.Fns.return_int()`). Reason: Java requires every method to
> live on a class. `sdkgen_java/src/emit.rs:324-343`, `lib.rs:217-237`.

> ⚠ **Deviation from Python:** If a package already contains a user type named
> `Fns`, the holder is emitted as **`Fns$`** (keyword/collision escape) so the
> two files never clash. Python has no analog because its functions are package
> attributes, not a named holder. `sdkgen_java/src/emit.rs:326-327`,
> `lib.rs:224-228` (test `fns_holder_escapes_on_user_class_collision`).

> ⚠ **Deviation from Python:** `async` is a **`CompletableFuture<T>` sibling
> method** (`make_foo_async`), and **sync is `.join()`**, not `asyncio`. The
> async body is a raw wildcard-bridge cast over `BamlFfi.callAsync` (deliberately
> **not** a `thenApply` stage, so `future.cancel(true)` still reaches the engine
> call). `sdkgen_java/src/emit.rs:533-590`.

> ⚠ **Deviation from Python:** Every binding carries a **trailing `ctx` overload**
> (`make_foo(v, ctx)` / `make_foo_async(v, ctx)`) taking a
> `baml_bridge.BamlCallContext` for cancellation — the Java analog of Python's
> `_ctx=` keyword, spelled as an overload because Java has no keyword args. `ctx`
> is always the last parameter. **DECIDED (D1, 2026-07-17)** and already emitted.
> `sdkgen_java/src/emit.rs:472-504,547-590`; runtime
> `baml_bridge/BamlCallContext.java`.

> ⚠ **Deviation from Python:** The compiled bytecode is a **base64 classpath
> resource `inlinedbaml.b64`** decoded at class-init, not a Python module holding
> a `bytes` literal (`_inlinedbaml.py`). Reason: the JVM caps a single static
> initializer at 64 KB of bytecode, so a `new byte[]{…}` literal cannot hold a
> real program. `sdkgen_java/src/lib.rs:11-13`; `Baml.java:450-460`.

> ⚠ **Deviation from Python:** The type map is registered by **`static {}`
> `registerClass` / `registerEnum` / `registerUnion` calls in `Baml.java`**,
> carrying the field declaration order and a parallel per-field decode-descriptor
> array; Python builds `BamlTypeMap.from_lazy_entries(...)` in a `_typemap.py`.
> `sdkgen_java/src/lib.rs:111-306`; real calls in `Baml.java:45-449`.

## Primitive Namespace

Derived from `ns_primitives/types.baml` and
`type_shapes/generated/baml_sdk/primitives`.

```java
// baml_sdk/primitives/Primitives.java
public final class Primitives {
    private final long int_field;
    private final double float_field;
    private final java.lang.String string_field;
    private final boolean bool_field;
    private final java.lang.Void null_field;
    private final byte[] uint8array_field;

    public Primitives(long int_field, double float_field, java.lang.String string_field,
                      boolean bool_field, java.lang.Void null_field, byte[] uint8array_field) { … }

    public long int_field()          { return this.int_field; }
    public double float_field()      { return this.float_field; }
    public java.lang.String string_field() { return this.string_field; }
    public boolean bool_field()      { return this.bool_field; }
    public java.lang.Void null_field()     { return this.null_field; }
    public byte[] uint8array_field() { return this.uint8array_field; }

    @Override public boolean equals(java.lang.Object o) {
        if (this == o) return true;
        if (!(o instanceof Primitives)) return false;
        Primitives other = (Primitives) o;
        return this.int_field == other.int_field
            && java.lang.Double.compare(this.float_field, other.float_field) == 0
            && java.util.Objects.equals(this.string_field, other.string_field)
            && this.bool_field == other.bool_field
            && java.util.Objects.equals(this.null_field, other.null_field)
            && java.util.Arrays.equals(this.uint8array_field, other.uint8array_field);
    }
    @Override public int hashCode() { … java.lang.Double.hashCode(this.float_field) … java.util.Arrays.hashCode(this.uint8array_field) … }
}
```

The BAML→Java scalar map: `int → long`, `float → double`, `string → String`,
`bool → boolean`, `null → java.lang.Void`, `bytes/uint8array → byte[]`
(`translate_ty.rs:85-100`).

```java
// baml_sdk/primitives/Fns.java  (pooled per-holder $RET constants; $RET0 = BamlType.INT)
private static final baml_bridge.BamlType $RET0 = baml_bridge.BamlType.INT;

public static long return_int() {
    return (java.lang.Long) baml_bridge.BamlFfi.callSync("user.primitives.return_int", new java.lang.String[] {}, new java.lang.Object[] {}, $RET0);
}
public static byte[] round_trip_uint8_array(byte[] b) {
    // bigint / uint8array / null are wire-driven → the descriptor is the literal null.
    return (byte[]) baml_bridge.BamlFfi.callSync("user.primitives.round_trip_uint8_array", new java.lang.String[] {"b"}, new java.lang.Object[] {b}, null);
}
```

> ⚠ **Deviation from Python:** Generated value types are **hand-emitted `public
> final` POJOs** (`private final` fields, canonical all-args constructor,
> accessors, deep `equals`/`hashCode`), **not Pydantic models and not Java
> `record`s**. Three consequences: (a) there is **no runtime validation** on
> construction — Pydantic validates fields, the Java constructor just assigns;
> (b) equality is deep and hand-written — `byte[]` uses
> `Arrays.equals`/`Arrays.hashCode` (a `record`'s array component would compare
> by identity, which the round-trip parity tests forbid), and **`double` fields
> use `Double.compare(a,b) == 0` / `Double.hashCode`** (landed `eab6d37cc`) so
> `-0.0`/`NaN` compare per the IEEE-total-order the parity tests expect, not `==`;
> (c) the class is **`final`** — the inbound encoder keys its typemap on the exact
> runtime class, so a user subclass would silently break inbound-encode.
> `sdkgen_java/src/emit.rs:137-302`.

> ⚠ **Deviation from Python:** Accessors are **`PreserveCase` zero-prefix
> methods** named exactly after the BAML field (`int_field()`, `uint8array_field()`),
> not attribute access (`p.int_field`). `sdkgen_java/src/emit.rs:208-213`.

> ⚠ **Deviation from Python:** The last positional argument to
> `callSync`/`callAsync` is a **type-directed decode descriptor** for the declared
> return type — a typed `baml_bridge.BamlType` **data structure**
> (`BamlType.INT`, `BamlType.classByFqn("user.Foo")`,
> `BamlType.union(BamlType.INT, BamlType.STRING)`), pooled per holder as a
> `private static final BamlType $RET{n}` constant — so the decoder resolves union
> arm order and element types without trusting the wire shape. A wholly
> wire-driven return (bigint / uint8array / null / void / media / callable /
> handle / the `unknown`-family) passes the literal `null`. The old
> stringly-typed grammar (`"int"`, `"union[int;string]"`) and its hand-rolled
> parser were **deleted** (`763a226ef`). Python's `.pyi` is the typed surface;
> Java threads the `BamlType` at the call. `emit.rs` (`DescriptorPool`, which
> interns each distinct builder expression into a per-holder `$RET{n}` constant),
> `translate_ty.rs` (`descriptor_expr` / `descriptor_expr_opt`).

## Enums And Aliases

Enums render as a plain Java `enum` with the BAML variant spelling preserved:

```java
// baml_sdk/enums/Sentiment.java
public enum Sentiment {
    Positive,
    Negative,
}
```

```java
// baml_sdk/enums/Enums.java (fields widen a variant type to the enum)
public final class Enums {
    private final baml_sdk.enums.Sentiment bare_enum;
    private final baml_sdk.enums.Sentiment variant_as_type;   // a specific variant type widens to Sentiment
    …
}
```

As in Python, a specific enum-variant type widens to the enum class
(`translate_ty.rs:123-125`, `Ty::Enum | Ty::EnumVariant`).

Non-recursive aliases **erase to their resolved type** (Java has no alias
mechanism): `StringList = string[]` never mints a type; a field typed `StringList`
is just `java.util.List<java.lang.String>` (`translate_ty.rs:126-134,30-31`).

A **recursive** alias cannot erase (erasure would not terminate), so it mints a
nominal **sealed interface** named after the alias, one `record` arm per union
member:

```java
// baml_sdk/aliases/RecList.java   —  BAML: type RecList = int | RecList[]
public sealed interface RecList permits RecList.IntValue, RecList.RecListListValue {
    record IntValue(java.lang.Long value) implements RecList {}
    record RecListListValue(java.util.List<baml_sdk.aliases.RecList> value) implements RecList {}
}
```

```java
// baml_sdk/aliases/AliasContainer.java
public final class AliasContainer {
    private final java.util.List<java.lang.String> list_field;   // StringList erased
    private final baml_sdk.aliases.RecList rec_field;            // recursive alias kept nominal
    …
}
```

> ⚠ **Deviation from Python:** Python renders non-recursive aliases as
> `typing.TypeAlias` and recursive ones as `typing_extensions.TypeAliasType` with
> quoted forward refs — the alias name survives in both cases. Java **erases
> non-recursive aliases entirely** and only mints a nominal type for **recursive**
> ones (as a sealed interface, not a type alias). `translate_ty.rs:24-31,126-134`;
> emitter `emit.rs:712-758` (`render_union`), `lib.rs:239-263`.

**Anonymous multi-arm unions** are where Java diverges most sharply. `int | string`
becomes the runtime's **generic arity family** `baml_bridge.Union2<Long, String>`,
a sealed interface with one `record Arm{i}` per positional arm:

```java
// baml_sdk/unions/UnionContainer.java (fields)
private final baml_bridge.Union2<java.lang.Long, java.lang.String> null_to_end;
private final baml_bridge.Union2<java.lang.Long, java.lang.String> dedup;
private final long singleton_unwrap;                                   // int|int → int (singleton unwrap)
private final baml_bridge.Union2<baml_sdk.unions.T, java.lang.String> optional_plus_null;
```

```java
// baml_bridge/Union2.java (runtime library, not generated per-fixture)
public sealed interface Union2<T0, T1> extends BamlUnion permits Union2.Arm0, Union2.Arm1 {
    record Arm0<T0, T1>(T0 value) implements Union2<T0, T1> {}
    record Arm1<T0, T1>(T1 value) implements Union2<T0, T1> {}
}
```

> ⚠ **Deviation from Python:** Python has no compile-time union type — a field is
> `typing.Union[int, str]` and the runtime Pydantic model discriminates at
> validation. Java has **no runtime union**, so an anonymous union renders as a
> **fixed member `Union2`..`Union10`** (`baml_bridge/Union2.java`…`Union10.java`),
> generic over the arm types in **declaration order** (null arm stripped), and
> **decode is type-directed** via the per-binding descriptor token
> (`union[int;string]`). Arity > 10 falls back to `java.lang.Object`.
> `translate_ty.rs:198-233` (`translate_union`), `304-347` (`descriptor_token`);
> arm family `baml_bridge/Union2.java:1-25`.

> ⚠ **Deviation from Python:** `singleton_unwrap` (`int | int` after
> normalization) is a bare `long`, and literal unions over one base erase to the
> base (`"draft" | "sent"` → `String`) — Java has no literal types.
> `translate_ty.rs:204-208,239-259`.

## Generics And Instance Methods

Derived from `ns_generics/types.baml`, `ns_generics/methods.baml`, and
`type_shapes/generated/baml_sdk/generics`.

Generic classes are Java generics (`class Wrapper<T>`); BAML methods are emitted
directly as instance/static methods on the class body:

```java
// baml_sdk/generics/WrapperMethods.java
public final class WrapperMethods<T> {
    static { baml_sdk.Baml.ensure(); }

    private final T value;
    public WrapperMethods(T value) { this.value = value; }
    public T value() { return this.value; }

    // Reified factory (explicit-generics surface, landed 861414d55): binds the
    // per-class-param type-arg tokens in the runtime side-table so the value
    // carries its concrete class_ty.type_args on the wire.
    public static <T> WrapperMethods<T> of(baml_bridge.BamlType $t0, T value) {
        WrapperMethods<T> $instance = new WrapperMethods<>(value);
        baml_bridge.TypeRegistry.bindTypeArgs($instance, java.util.List.of($t0));
        return $instance;
    }
    // Reads those tokens back (delegates to the weak-identity side-table).
    public java.util.List<baml_bridge.BamlType> bamlTypeArgs() {
        return baml_bridge.TypeRegistry.typeArgsOf(this);
    }
    private static final baml_bridge.BamlType $RET0 = baml_bridge.BamlType.typeVar("T");

    public T get_value() {
        return (T) baml_bridge.BamlFfi.callSync("user.generics.WrapperMethods.get_value", new java.lang.String[] {"self"}, new java.lang.Object[] {this}, $RET0);
    }
    @SuppressWarnings("unchecked")
    public java.util.concurrent.CompletableFuture<T> get_value_async() { … $RET0 … }

    // A union with a TypeVar arm (`T | WrapperMarker`) degenerates to
    // java.lang.Object with a null (wire-driven) descriptor — no Union2 is minted
    // when an arm is an unresolved type var (translate_ty.rs:236-250).
    public java.lang.Object get_value_or_marker() {
        return (java.lang.Object) baml_bridge.BamlFfi.callSync("user.generics.WrapperMethods.get_value_or_marker", new java.lang.String[] {"self"}, new java.lang.Object[] {this}, null);
    }
    // + get_value(ctx) / get_value_async(ctx) overloads
    // equals/hashCode use WrapperMethods<?> wildcard narrowing (erasure)
}
```

Instance methods prepend the receiver (`"self"` name / `this` arg) to the runtime
call so the engine sees it as required param 0 (`emit.rs:348-353,414-421`). The
`self` receiver never appears in the Java signature. Nested generics
(`Box<T>` with a `Wrapper<T>` field) carry through as ordinary Java type
arguments:

```java
// baml_sdk/generics/Box.java
public final class Box<T> {
    private final T value;
    private final baml_sdk.generics.Wrapper<T> wrapped;
    …
}
```

Free-function generics declare their type parameters on the Java method and
**infer the type args engine-side** — the arg carries the value, the descriptor
carries `tv:T`:

```java
// baml_sdk/generic_tests/Fns.java   ($RET0 = BamlType.typeVar("T"))
public static <T> T identity(T x) {
    return (T) baml_bridge.BamlFfi.callSync("user.generic_tests.identity", new java.lang.String[] {"x"}, new java.lang.Object[] {x}, $RET0);
}
// Explicit-generics trailing overloads (landed 861414d55) — pass a BamlTypes bag
// via a 6-arg callSync/callAsync (returnDesc, ctx, types):
public static <T> T identity(T x, baml_bridge.BamlTypes types) {
    return (T) baml_bridge.BamlFfi.callSync("user.generic_tests.identity", new java.lang.String[] {"x"}, new java.lang.Object[] {x}, $RET0, null, types);
}
public static <T> T identity(T x, baml_bridge.BamlTypes types, baml_bridge.BamlCallContext ctx) { … $RET0, ctx, types … }

public static <A, B, C> baml_sdk.generic_tests.GenericTriple<A, B, C> make_triple(
        A a, java.util.List<B> b, java.util.Map<java.lang.String, C> c) { … }
```

A static factory that would collide with the Java `new` keyword is escaped, and
carries the same `BamlTypes` trailing overloads:

```java
// baml_sdk/generic_tests/GenericBox.java  —  BAML static `new`  ($RET0 = classByFqn("user.generic_tests.GenericBox"))
public static <T, V> baml_sdk.generic_tests.GenericBox<V> new$(V value) {
    return (baml_sdk.generic_tests.GenericBox<V>) baml_bridge.BamlFfi.callSync("user.generic_tests.GenericBox.new", new java.lang.String[] {"value"}, new java.lang.Object[] {value}, $RET0);
}
public static <T, V> baml_sdk.generic_tests.GenericBox<V> new$(V value, baml_bridge.BamlTypes types) {
    return (baml_sdk.generic_tests.GenericBox<V>) baml_bridge.BamlFfi.callSync("user.generic_tests.GenericBox.new", new java.lang.String[] {"value"}, new java.lang.Object[] {value}, $RET0, null, types);
}
```

An **instance** generic method with a `BamlTypes` overload guards on a reified
receiver — `GenericBox.pair_with(other, types)` throws
`IllegalArgumentException("explicit type bindings on a generic method require a
reified receiver so the class type args can be recovered")` when
`TypeRegistry.typeArgsOf(this).isEmpty()`.

> ⚠ **Deviation from Python:** A static method on a generic class **re-declares
> the class's type params at method level** (`static <T, V> … new$`), because
> Java statics cannot reference class-level type variables. Python's
> `staticmethod` sits inside `Generic[T]` and needs no re-declaration.
> `emit.rs:446-465`.

> ⚠ **Deviation from Python:** A BAML method/factory whose name is a Java reserved
> word is `$`-escaped (`new` → `new$`, and its async sibling `new$_async`).
> Python only escapes `$` (e.g. `$stream` → base name); Java escapes the JLS
> keyword set. `routing.rs:119-133`; real output `GenericBox.java:24-40`.

**Explicit type-argument bindings** (Python's `identity[int](5)` subscript and
low-level `_types={T: int}` kwarg, plus the `class_type_params=[…]` /
`type_params=[…]` metadata on `define_function`):

**LANDED** (commit `861414d55`, on top of the `3991c4fd4` runtime substrate). The
full explicit-generics emitter surface now ships in `generated/baml_sdk/**`:

- **Runtime substrate** (`3991c4fd4`): the value-level type tokens in
  `baml_bridge/BamlType.java` (`INT`/`STRING`/`BOOL`/`FLOAT`, `of(Class)`,
  `of(Class, BamlType...)`, `classByFqn`, `typeVar`, `toWireTy`/`fromWireTy`) and
  `baml_bridge/BamlTypes.java` (`BamlTypes.of("T", BamlType.INT).and(…)`, an
  ordered named bag).
- **Call-site surface (D3):** a **named bag** `BamlTypes.of("T", BamlType.INT)`
  passed as a **trailing overload** (1:1 with the wire's named `BamlTyArg`
  bindings; partial binding allowed) through a 6-arg
  `callSync`/`callAsync(fqn, names, args, returnDesc, ctx, types)`. Emitted for
  free functions, static factories, and instance methods (see `identity`,
  `GenericBox.new$`, `GenericBox.pair_with` above).
- **Reified factory + readback:** every generic class gets a static
  `of(BamlType $t0, …, T value)` factory (binds the class type args into the
  side-table) and a **`bamlTypeArgs()`** accessor (reads them back). This is the
  resolved answer to the erstwhile-open readback-naming question — the accessor is
  spelled `bamlTypeArgs()`, delegating to `TypeRegistry.typeArgsOf`.
- **Instance-method guard:** a generic *instance* method's `BamlTypes` overload
  throws `IllegalArgumentException` unless the receiver was reified (its class
  type args must be recoverable) — `GenericBox.pair_with(other, types)` above.
- **Still minimal-grammar** (unchanged): a reified arg outside the token grammar
  (list/map/union/optional/literal) yields no side-table entry
  (`BamlType.fromWireTy` returns `null` out of grammar) — graceful degradation,
  not an error.
- Confirmed present in generated output: `grep -rl BamlTypes generated/baml_sdk/**`
  now matches the generic holders/classes; `GenericBox.java` carries the reified
  `of(BamlType, value)` factory and `bamlTypeArgs()`.

## Cross-Namespace References

Derived from `ns_symbol_collisions/ns_lorem/uses.baml`.

Cross-package field types are **fully-qualified references** — Java allows an FQN
in any type position, so no import machinery is needed and the namespace boundary
is preserved by the package path itself:

```java
// baml_sdk/symbol_collisions/lorem/Ipsum.java
package baml_sdk.symbol_collisions.lorem;

public final class Ipsum {
    private final baml_sdk.symbol_collisions.foo.Bar bar1;
    private final baml_sdk.symbol_collisions.fizz.foo.Bar bar2;
    private final baml_sdk.symbol_collisions.fizz.buzz.foo.Bar bar3;

    public baml_sdk.symbol_collisions.foo.Bar bar1() { return this.bar1; }
    public baml_sdk.symbol_collisions.fizz.foo.Bar bar2() { return this.bar2; }
    public baml_sdk.symbol_collisions.fizz.buzz.foo.Bar bar3() { return this.bar3; }
    …
}
```

The boundary is preserved: `Ipsum` lives at
`baml_sdk.symbol_collisions.lorem.Ipsum`, and its distinct `foo.Bar` neighbours
stay at their own packages. Callers reach it via
`baml_sdk.symbol_collisions.lorem.Fns.make_ipsum(bar1, bar2, bar3)`.

> ⚠ **Deviation from Python:** No conditional / `TYPE_CHECKING`-guarded imports
> and no forward-reference quoting. Python's runtime `__init__.py` imports the
> nearest package so Pydantic can resolve annotations at schema-build time, and
> the `.pyi` guards that import under `typing.TYPE_CHECKING`. Java references
> every cross-package type by FQN, deleting the whole import-collection machinery.
> `translate_ty.rs:1-9,179-193` (`qualified_type`).

## Stdlib Re-Exports

Most generated symbols are POJOs, enums, minted unions, or `Fns` bindings. Two
kinds of stdlib symbols are **runtime-owned** rather than generated per fixture.

**Media** (`baml.media.Image` / `Audio` / `Video` / `Pdf`) are handle-backed
classes provided by the runtime library at exactly their public package path;
`translate_ty` maps the media type straight to `baml_sdk.baml.media.Image` and no
class is generated for it (`translate_ty.rs:101-108`; `lib.rs` test asserts
`baml/media/Image.java` is *not* in generated output while `baml/http/Response.java`
*is*).

```java
// baml_bridge/src/main/java/baml_sdk/baml/media/Image.java (runtime-owned, hand-written)
public final class Image implements BamlMedia {
    public static Image from_url(String url, String mimeType) { … }
    public static Image from_file(String path, String mimeType) { … }
    public static Image from_base64(String base64, String mimeType) { … }
    public String url()  { return handle.mediaUrl(); }
    public String file() { return handle.mediaFile(); }
    public String base64() { return handle.mediaBase64(); }
    public String mime_type() { return handle.mediaMimeType(); }   // PreserveCase: NOT mimeType()
}
```

> ⚠ **Deviation from Python:** The media accessor is **`mime_type()`**
> (snake_case, `PreserveCase` parity with `test_handles.py`), not the
> Java-idiomatic `mimeType()`. `baml_sdk/baml/media/Image.java:73`.

**LLM function operations** use one opaque spec capability rather than generated
synthetic functions. For an authored LLM function `extract_resume`, codegen emits
the authored direct binding, a flat `extract_resume_spec` factory, and—when the
operation metadata says the function is streamable—a flat `extract_resume_stream`
shortcut. The spec carries the final type; the stream binding carries the retained
PPIR partial-output type and the final type:

```java
// baml_sdk/lorem/Fns.java (conceptual excerpt)
public static baml_bridge.BamlFunctionSpec<Resume>
extract_resume_spec(String text) { … }

public static baml_bridge.BamlStream<Resume$stream, Resume>
extract_resume_stream(String text) { … }
```

The factory sends `BamlFunctionOperation.SPEC` on the authored function FQN.
The stream shortcut sends `BamlFunctionOperation.STREAM` on the same authored
FQN, forwarding the generated `client` and `on_event` controls; the engine
resolves that boundary request to PPIR's private ordinary
`extract_resume@stream`. Prompt rendering, request construction, parsing, and
direct invocation are methods on the one-generic capability:

```java
BamlFunctionSpec<Resume> spec = Fns.extract_resume_spec(text);
BamlPrompt prompt = spec.prompt();
Object request = spec.build_request();
Resume parsed = spec.parse(json);
Resume called = spec.call();
BamlStream<Resume$stream, Resume> stream =
    Fns.extract_resume_stream(text, client, onEvent);
```

`ai.Prompt` maps directly to the runtime-owned `BamlPrompt` host type. It stores
the portable prompt protobuf payload rather than an engine handle, so repeated
`text()` and `messages()` calls each re-enter BAML from the same payload. There
is no generated prompt twin or conversion-only wrapper.

The runtime-owned `Stream` is re-exported: `ai.stream.Stream` resolves to
`baml_bridge.BamlStream` (`translate_ty.rs:180-186`).

> ⚠ **Deviation from Python:** Python re-exports handle-backed types with import
> aliases (`from baml_bridge.baml_py import BamlPdf as Pdf`; `from baml_bridge
> import BamlStream as Stream`). Java has no re-export shim — `translate_ty`
> resolves the type name **directly** to the runtime class
> (`baml_bridge.BamlStream`, `baml_sdk.baml.media.Image`). `translate_ty.rs:179-193`.

**Streaming calls / `Stream`**: **LANDED** (commit `a6e3ca99e`, the streaming
capability). `baml_bridge/BamlStream.java` is a **real**
`BamlStream<TPartial, TFinal>` wrapping the tagged-heap handle:
`next()` / `get_final()` (and their `_async` siblings) re-enter the engine via
ordinary `BamlFfi.callSync`/`callAsync`. Decode retains the tagged handle's
concrete `ty.class_ty.name`; the wrapper derives `<FQN>.next` / `<FQN>.final`
from that identity and passes `this` as the `self` receiver with a **`null`
(wire-driven) descriptor**. Generated flat `_stream` shortcuts return
`baml_bridge.BamlStream<TPartial, TFinal>` through the authored-FQN Stream
boundary operation and PPIR's private `Fn@stream`. Exhaustion returns a
runtime-owned `baml_sdk.ai.stream.Done` **value** (no `null`, no exception),
registered in the typemap under `ai.stream.Done`. Decode maps
`ADT_TAGGED_HEAP_HANDLE` to `BamlStream.fromHandle` after retaining its class
FQN; encode delegates to the `BamlHandle` arm (cloned key per the drain
contract). `final`/`final_async`
escape to `get_final`/`get_final_async` (Java reserved word; OWNER decision
2026-07-18). (See `ref-java-outbound-decoding.md` "Handles" and `ref-java-type-mappings.md`.)

`build_request()` is a method on `BamlFunctionSpec`, not a generated sibling
function. Likewise, `prompt()`, `parse(...)`, and `call(...)` have one canonical
implementation on the spec. Streaming uses the flat `_stream` operation binding;
generated code never fabricates `$spec`, `$stream`, `$parse`, `$render_prompt`,
or `$build_request` callable names.

## Stream Partial-Output Types

Java keeps the compiler-produced PPIR `$stream` partial output as an **in-package class
with the `$stream` name kept verbatim** — every optional-widened field boxes:

```java
// baml_sdk/primitives/Primitives$stream.java
public final class Primitives$stream {
    private final java.lang.Long int_field;       // int   → boxed Long   (partial ⇒ nullable)
    private final java.lang.Double float_field;    // float → boxed Double
    private final java.lang.String string_field;
    private final java.lang.Boolean bool_field;
    private final java.lang.Void null_field;
    private final byte[] uint8array_field;
    …
}
```

The type map routes both the base and the partial-output FQN to their in-package Java
classes (from `Baml.java`):

```java
registerClass("user.primitives.Primitives",        "baml_sdk.primitives.Primitives",        …);
registerClass("user.primitives.Primitives$stream", "baml_sdk.primitives.Primitives$stream", …);
```

Generic and recursive partial-output models follow the same in-package convention
(`generics/WrapperMethods$stream.java`, `aliases/RecList$stream.java`). As in
Python, codegen consumes the compiler-produced `$stream` class shape as a regular
class; it does not derive a `Partial[T]` transformation at Java codegen time.

> ⚠ **Deviation from Python:** Python puts partial-output models in a **parallel
> `stream_types/<ns>` package** and names the class the base name (because `$` is
> not a Python identifier); the type map maps `…$stream` FQN → `stream_types`
> module. Java keeps `<Name>$stream` **beside its base type** (no parallel tree).
> `routing.rs:19-23,141-158` (routing ignores the `$stream` suffix). *Note (GAP B,
> handoff):* the ported `TestStreams` tests were written to Python's
> `stream_types.*` layout; DECIDED 2026-07-17 (Option B): tests were retargeted to
> `$stream`, house-rule, vs move the emitter to parallel packages).

## Optional Function Arguments

Derived from `function_calls/baml_src/main.baml` and
`function_calls/generated/baml_sdk/Fns.java`.

Required arguments stay positional. Optional (defaulted) BAML arguments do **not**
become positional Java params; instead a **trailing configurator overload** is
emitted beside the required-only pair — an AWS-SDK-v2-style
`Consumer<<Ident>$Opts>` plus a nested `<Ident>$Opts` fluent options class:

```java
// baml_sdk/Fns.java  —  BAML: optional_args_probe(arg0: int, opt1?: int = 5, opt2?: int)
//   $RET6 = baml_bridge.BamlType.list(baml_bridge.BamlType.INT)
public static java.util.List<java.lang.Long> optional_args_probe(long arg0) {
    return (java.util.List<java.lang.Long>) baml_bridge.BamlFfi.callSync("user.optional_args_probe", new java.lang.String[] {"arg0"}, new java.lang.Object[] {arg0}, $RET6);
}

public static java.util.List<java.lang.Long> optional_args_probe(
        long arg0, java.util.function.Consumer<optional_args_probe$Opts> $cfg) {
    optional_args_probe$Opts $opts = new optional_args_probe$Opts();
    $cfg.accept($opts);
    return (java.util.List<java.lang.Long>) baml_bridge.BamlFfi.callSync("user.optional_args_probe", $opts.$names(new java.lang.String[] {"arg0"}), $opts.$args(new java.lang.Object[] {arg0}), $RET6);
}

public static final class optional_args_probe$Opts {
    private final java.util.LinkedHashMap<java.lang.String, java.lang.Object> $values = new java.util.LinkedHashMap<>();
    private final java.util.LinkedHashSet<java.lang.String> $touched = new java.util.LinkedHashSet<>();

    public optional_args_probe$Opts opt1(java.lang.Long v) { this.$values.put("opt1", v); this.$touched.add("opt1"); return this; }
    public optional_args_probe$Opts opt2(java.lang.Long v) { this.$values.put("opt2", v); this.$touched.add("opt2"); return this; }

    java.lang.String[] $names(java.lang.String[] base) { /* append only $touched names */ }
    java.lang.Object[] $args(java.lang.Object[] base)  { /* append only $touched values */ }
}
```

Call site — the tri-state (omit vs explicit null vs value) is preserved with no
sentinel:

```java
Fns.optional_args_probe(3);                          // omit both → engine evaluates BAML defaults
Fns.optional_args_probe(3, o -> o.opt1(9L));         // opt1 touched → sent; opt2 omitted → default
Fns.optional_args_probe(3, o -> o.opt1(null));       // opt1 touched-with-null → explicit BAML null
```

Static and instance overloads carry the same pattern (`OptBox.make(base, $cfg)`,
`OptBox.probe(arg0, $cfg)`), and each configurator overload also gets its own
trailing-`ctx` sibling.

> ⚠ **Deviation from Python:** Python makes optionals **keyword-only params with
> an `UNSET` sentinel** (`opt1: Union[int, None, UNSET] = 5`), and `_build_kwargs`
> drops any arg that `is UNSET`. Java has no keyword args or sentinels, so it
> uses a **fluent `$Opts` bag**: an *untouched* optional is simply absent from the
> wire arrays (engine default), a *touched* optional is appended by name — and a
> touched-with-`null` optional sends explicit BAML `null`. Same tri-state,
> different mechanism. `sdkgen_java/src/emit.rs:592-707` (`render_optional_configurator`);
> `Fns.java:168-233`, `OptBox.java:42-180`.

> ⚠ **Deviation from Python:** Java does **not** emit the literal default value
> (`= 5`) anywhere — untouched *always* means "let the engine evaluate the BAML
> default," so literal and expression defaults are handled uniformly by omission.
> Python emits the literal default into the `.pyi` (`opt1 … = 5`) and only uses
> `UNSET` for expression defaults. `emit.rs:592-606`.

## Static And Instance Methods

Derived from `function_calls/baml_src/ns_methods_on_classes/types.baml`.

```java
// baml_sdk/methods_on_classes/Greeter.java
public final class Greeter {
    static { baml_sdk.Baml.ensure(); }

    private final java.lang.String name;
    public Greeter(java.lang.String name) { this.name = name; }
    public java.lang.String name() { return this.name; }
    private static final baml_bridge.BamlType $RET0 = baml_bridge.BamlType.classByFqn("user.methods_on_classes.Greeter");
    private static final baml_bridge.BamlType $RET1 = baml_bridge.BamlType.STRING;

    public static baml_sdk.methods_on_classes.Greeter create(java.lang.String name) {
        return (baml_sdk.methods_on_classes.Greeter) baml_bridge.BamlFfi.callSync("user.methods_on_classes.Greeter.create", new java.lang.String[] {"name"}, new java.lang.Object[] {name}, $RET0);
    }
    @SuppressWarnings("unchecked")
    public static java.util.concurrent.CompletableFuture<baml_sdk.methods_on_classes.Greeter> create_async(java.lang.String name) { … }

    public java.lang.String who() {                       // instance method: receiver prepended
        return (java.lang.String) baml_bridge.BamlFfi.callSync("user.methods_on_classes.Greeter.who", new java.lang.String[] {"self"}, new java.lang.Object[] {this}, $RET1);
    }
    public java.lang.String greet(java.lang.String greeting) {
        return (java.lang.String) baml_bridge.BamlFfi.callSync("user.methods_on_classes.Greeter.greet", new java.lang.String[] {"self", "greeting"}, new java.lang.Object[] {this, greeting}, $RET1);
    }
    // + _async siblings and (…, ctx) overloads for each
}
```

Static methods are `static` bindings (same shape as free functions); instance
methods are non-static and prepend the receiver (`"self"` name / `this` arg) so
the engine sees it as required param 0. The method's binding FQN is
`<class fqn>.<method name>`. There is no hand-written method body delegating to a
separate binding — the method *is* the binding (`emit.rs:216-251,348-353`).

> ⚠ **Deviation from Python:** Python uses `staticmethod(define_function(...))`
> descriptors and lists `"self"` in the runtime parameter-name array while hiding
> it from the `.pyi`; Java achieves the same split structurally — a plain `static`
> vs instance method, with `self`/`this` prepended only into the runtime arrays,
> never the Java signature. `emit.rs:412-421`.

## Throws Javadoc  *(Python: Throws Docstrings)*

Derived from `function_calls/baml_src/ns_raises_test/types.baml` and
`function_calls/generated/baml_sdk/raises_test`.

Thrown types are **documented, not encoded in the return type or as checked
exceptions** — the JVM has no checked-exception representation for BAML throws,
so the contract lands as Javadoc `@throws` tags (one per thrown type, source
order), and the signature still returns the success type:

```java
// baml_sdk/raises_test/Fns.java
/**
 * Load a document from a path.
 *
 * @throws ParseError
 * @throws TimeoutError
 */
public static java.lang.String LoadDoc(java.lang.String path) {
    return (java.lang.String) baml_bridge.BamlFfi.callSync("user.raises_test.LoadDoc", new java.lang.String[] {"path"}, new java.lang.Object[] {path}, $RET0);  // $RET0 = BamlType.STRING
}
```

An inferred throws contract (a BAML function with no written `throws` clause)
still renders `@throws` tags (`InferredThrow` → `@throws ParseError`); a pure
non-throwing function renders only its summary or no comment
(`PureLen` — `emit.rs:116-135`, `collect_raises_names`). Class methods carry the
same tags (`raises_test/DocLoader.java`: `create()` → `@throws TimeoutError`,
`load(path)` → `@throws ParseError`).

> ⚠ **Deviation from Python:** Python attaches the contract as a `Raises:` block
> inside the function's `__doc__` / `.pyi` docstring; Java uses structured
> **Javadoc `@throws <UnqualifiedName>`** tags shared by the sync binding, its
> `_async` sibling, and any configurator overloads. `emit.rs:56-108,434-439`.

At runtime a thrown value surfaces as an unchecked wrapper (there is no BAML type
that is itself a `Throwable`): `baml_bridge.BamlError` (extends `RuntimeException`)
carries the decoded value on `.value()` with `.baml_trace()` / `.class_name()`
(`BamlError.java`).

> ⚠ **Deviation from Python (error mapping, D2 — runtime, partially landed):**
> `baml.errors.TypeMismatch` remaps to `IllegalArgumentException` (the only
> remap, 1:1 with Python's `TypeError`); BAML trace frames are synthesized into
> real `StackTraceElement`s; and **`BamlPanic` re-parents to `Error`**
> (`BamlPanic.java:20`) — the JVM analog of Python's `BamlPanic` subclassing
> `BaseException`, so a bare `catch (Exception)` does not swallow a panic.
> Async engine-abort cancellation surfaces as `BamlCancelledError extends
> CancellationException` (`isCancelled()==true`, unwrapped from `join()`/`get()`);
> sync cancel stays `BamlPanic(Cancelled)` (`BamlCancelledError.java:26-54`).

## Host Callable Types

Derived from `function_calls/baml_src/ns_host_callable_tests/main.baml`.

The **parameter types** for host callables **are emitted and compile today**:
`translate_callable` maps arity/return onto `java.util.function.*`
(`translate_ty.rs:403-435`):

```java
// baml_sdk/host_callable_tests/Fns.java   ($RET0 = BamlType.STRING)
public static java.lang.String call_with_callback(
        java.util.function.Function<java.lang.Long, java.lang.String> callback, long x) {
    return (java.lang.String) baml_bridge.BamlFfi.callSync(
        "user.host_callable_tests.call_with_callback",
        new java.lang.String[] {"callback", "x"}, new java.lang.Object[] {callback, x}, $RET0);
}
public static java.lang.String call_with_two_args(
        java.util.function.BiFunction<java.lang.Long, java.lang.String, java.lang.String> callback, long x, java.lang.String prefix) { … }
public static java.lang.String call_with_class_callback(
        java.util.function.Function<baml_sdk.host_callable_tests.Person, java.lang.String> callback, baml_sdk.host_callable_tests.Person p) { … }
```

A propagating typed throw is documented with a `@throws` tag, same as any other
function (mirrors Python's `Raises:` docstring):

```java
/**
 * @throws ValidationError
 */
public static java.lang.String call_with_typed_throws_propagating(
        java.util.function.Function<java.lang.Long, java.lang.String> callback, long x) { … }
```

> ⚠ **Deviation from Python:** Python renders a callable param as
> `typing.Callable[[int], str]`; Java maps by arity/return onto the concrete
> `Runnable` / `Supplier` / `Consumer` / `Function` / `BiConsumer` / `BiFunction`
> shapes. A callable with **optional params or arity > 2** — which has no
> `java.util.function` equivalent — is emitted as a **generated
> `@FunctionalInterface`** (landed `202883518`), not `java.lang.Object`: the
> `call_callback_with_optional_args_*` bindings take a
> `baml_sdk.host_callable_tests.IntOptCallback callback` — a fixed-arity SAM
> `Long apply(Long x, Opts $opts)` extending `baml_bridge.BamlHostCallable`, with
> a nested always-non-null `Opts` bag (nullable accessors for the optionals BAML
> omitted). `translate_ty.rs:403-435` (arity-≤-2 `java.util.function`),
> `translate_callable` fallback + emitter (`IntOptCallback.java`).

**Runtime host-callable dispatch (the engine calling back *into* the host):**
**LANDED** (commit `202883518`, per the owner-accepted A1–F1 brief). The whole
slice is wired end-to-end — `function_calls` is 154/0:

- **Registry (A1):** a Java-side `ConcurrentHashMap<Long,Object>` in `BamlFfi`
  holds callables *and* opaque throwables under one keyspace
  (`registerHostCallable` / `lookupHostValue` / `hostRelease`,
  `BamlFfi.java:582-616`); Rust is a pure router. `assertSame` identity is free
  (objects never leave the JVM).
- **Executor (B1):** a dedicated cached **daemon `ExecutorService`**
  (`HOST_DISPATCH_EXECUTOR`, `BamlFfi.java:104-105`) runs user callables off the
  engine's tokio workers; the C callback returns promptly and the result flows
  back via `nativeCompleteHostCall` (`BamlFfi.java:221, 626-801`).
- **Async detection (C1):** a returned `CompletableFuture` is detected at the
  **value level** and awaited (`.whenComplete`) instead of encoded inline — the
  async parity landed too (`a6e3ca99e`).
- **Exception identity (D):** a native exception thrown inside a callable becomes
  an opaque `baml.errors.HostCallable` handle (`HOST_VALUE_OPAQUE`) that
  round-trips the **same** `Throwable` by registry-key rehydration; `class_name` =
  `getSimpleName()` (see `ref-java-outbound-decoding.md`, error-arm rehydrate).
- **`IntOptCallback` (E1):** optional/high-arity callables emit a generated
  `@FunctionalInterface extends baml_bridge.BamlHostCallable` + nested non-null
  `Opts` bag, with a `default __bamlDispatch(...)` that reshapes the bridge's flat
  declared-order args into the SAM (below).
- **`BamlError(Object value)` ctor (F):** the throw-direction 1-arg ctor with
  typed unwrap exists so BAML's typed `catch` matches.

```java
// baml_sdk/host_callable_tests/IntOptCallback.java  (optional-arg callable → generated SAM)
@FunctionalInterface
public interface IntOptCallback extends baml_bridge.BamlHostCallable {
    java.lang.Long apply(java.lang.Long x, Opts $opts);

    @Override
    default java.lang.Object __bamlDispatch(java.util.List<java.lang.Object> $positional, java.util.Map<java.lang.String, java.lang.Object> $optional) {
        return apply((java.lang.Long) $positional.get(0), new Opts((java.lang.Long) $optional.get("y"), (java.lang.Long) $optional.get("z")));
    }

    final class Opts {                       // always constructed non-null; each accessor is null when BAML omitted it
        private final java.lang.Long y; private final java.lang.Long z;
        Opts(java.lang.Long y, java.lang.Long z) { this.y = y; this.z = z; }
        public java.lang.Long y() { return this.y; }
        public java.lang.Long z() { return this.z; }
    }
}
```

**Forced divergence recorded:** the engine requires the `HostCallable` traceback
field to be **present**, so Java always synthesizes one (Python's is
conditionally-always via `__traceback__`). The generated `Person` /
`ValidationError` value classes exist (`host_callable_tests/Person.java`,
`ValidationError.java`) as `public final` POJOs.

## Packaging Markers

The generated Java SDK root includes:

- **`inlinedbaml.b64`** — the compiler-produced bytecode as a base64 classpath
  resource, decoded in `Baml.java`'s static initializer via
  `Base64.getMimeDecoder()` (`lib.rs:11-13,320-326`; `Baml.java:450-460`);
- **`Baml.java`** — the runtime anchor whose `static {}` block registers the type
  map (`registerClass` / `registerEnum` / `registerUnion`, carrying field
  declaration order + per-field decode descriptors) and calls
  `BamlFfi.initFromBytecode(...)` (`lib.rs:111-306`); every `Fns` holder forces
  it via `static { Baml.ensure(); }`.

> ⚠ **Deviation from Python:** Python ships three packaging markers — a
> `_inlinedbaml.py` (bytecode as a Python `bytes` literal), a `_typemap.py`
> (`BamlTypeMap.from_lazy_entries(...)`), and a `py.typed` PEP 561 marker. Java
> folds the first two into `inlinedbaml.b64` + the `Baml.java` static block, and
> has **no `py.typed` analog** — a compiled `.class` on the classpath is
> inherently typed. Dependency-wise the generated code needs only `baml-bridge`
> on the classpath (Python's fixtures depend on `baml_bridge`, `pydantic>=2`, and
> `typing-extensions`). `lib.rs:6-24`.
