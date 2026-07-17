---
date: 2026-07-17
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
is inferred. Line numbers are as of 2026-07-17.

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
baml_sdk/Foo$stream.java                    # stream companion, in-package
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

> ⚠ **Deviation from Python:** `$stream` companions stay **in the base type's
> package** with a `$stream`-suffixed name (`Foo$stream`, `primitives/Primitives$stream`),
> because `$` is a legal Java identifier character. Python cannot use `$` in an
> identifier, so it emits a **parallel `stream_types/<ns>` package** and routes
> the `$stream` FQN there via the type map. Reason: keep the BAML name verbatim;
> no parallel package tree. `sdkgen_java/src/routing.rs:19-23`. (See
> *Stream Companion Types*.)

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
        baml_bridge.TypeRegistry.registerClass("user.primitives.Primitives", "baml_sdk.primitives.Primitives",
            new java.lang.String[] {"int_field", "float_field", "string_field", "bool_field", "null_field", "uint8array_field"},
            new java.lang.String[] {"int", "float", "string", "bool", "null", "uint8array"});
        // … registerClass / registerEnum / registerUnion for every symbol …
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

    public static baml_sdk.Foo make_foo(long v) {
        return (baml_sdk.Foo) baml_bridge.BamlFfi.callSync(
            "user.make_foo", new java.lang.String[] {"v"}, new java.lang.Object[] {v}, "user.Foo");
    }

    @SuppressWarnings("unchecked")
    public static java.util.concurrent.CompletableFuture<baml_sdk.Foo> make_foo_async(long v) {
        return (java.util.concurrent.CompletableFuture<baml_sdk.Foo>) (java.util.concurrent.CompletableFuture<?>)
            baml_bridge.BamlFfi.callAsync("user.make_foo", new java.lang.String[] {"v"}, new java.lang.Object[] {v}, "user.Foo");
    }

    // + trailing-`ctx` overloads make_foo(v, ctx) / make_foo_async(v, ctx); round_trip_foo(...) similarly
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
public class Primitives {
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
            && this.float_field == other.float_field
            && java.util.Objects.equals(this.string_field, other.string_field)
            && this.bool_field == other.bool_field
            && java.util.Objects.equals(this.null_field, other.null_field)
            && java.util.Arrays.equals(this.uint8array_field, other.uint8array_field);
    }
    @Override public int hashCode() { … java.util.Arrays.hashCode(this.uint8array_field) … }
}
```

The BAML→Java scalar map: `int → long`, `float → double`, `string → String`,
`bool → boolean`, `null → java.lang.Void`, `bytes/uint8array → byte[]`
(`translate_ty.rs:85-100`).

```java
// baml_sdk/primitives/Fns.java
public static long return_int() {
    return (java.lang.Long) baml_bridge.BamlFfi.callSync(
        "user.primitives.return_int", new java.lang.String[] {}, new java.lang.Object[] {}, "int");
}
public static byte[] round_trip_uint8_array(byte[] b) {
    return (byte[]) baml_bridge.BamlFfi.callSync(
        "user.primitives.round_trip_uint8_array", new java.lang.String[] {"b"}, new java.lang.Object[] {b}, "uint8array");
}
```

> ⚠ **Deviation from Python:** Generated value types are **hand-emitted POJOs**
> (`private final` fields, canonical all-args constructor, accessors, deep
> `equals`/`hashCode`), **not Pydantic models and not Java `record`s**. Two
> consequences: (a) there is **no runtime validation** on construction — Pydantic
> validates fields, the Java constructor just assigns; (b) equality is deep and
> hand-written — `byte[]` uses `Arrays.equals`/`Arrays.hashCode` (a `record`'s
> array component would compare by identity, which the round-trip parity tests
> forbid). `sdkgen_java/src/emit.rs:137-302`.

> ⚠ **Deviation from Python:** Accessors are **`PreserveCase` zero-prefix
> methods** named exactly after the BAML field (`int_field()`, `uint8array_field()`),
> not attribute access (`p.int_field`). `sdkgen_java/src/emit.rs:208-213`.

> ⚠ **Deviation from Python:** The last argument to `callSync`/`callAsync` is a
> **type-directed decode descriptor** for the declared return type (`"int"`,
> `"user.Foo"`, `"union[int;string]"`), so the decoder resolves union arm order
> and element types without trusting the wire shape. Python's `.pyi` is the typed
> surface; Java threads the descriptor at the call. `emit.rs:22-27`,
> `translate_ty.rs:304-347` (`descriptor_token`).

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
public class Enums {
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
public class AliasContainer {
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
public class WrapperMethods<T> {
    private final T value;
    public WrapperMethods(T value) { this.value = value; }
    public T value() { return this.value; }

    public T get_value() {
        return (T) baml_bridge.BamlFfi.callSync(
            "user.generics.WrapperMethods.get_value",
            new java.lang.String[] {"self"}, new java.lang.Object[] {this}, "tv:T");
    }
    @SuppressWarnings("unchecked")
    public java.util.concurrent.CompletableFuture<T> get_value_async() { … "tv:T" … }

    public baml_bridge.Union2<T, baml_sdk.generics.WrapperMarker> get_value_or_marker() {
        return (baml_bridge.Union2<T, baml_sdk.generics.WrapperMarker>) baml_bridge.BamlFfi.callSync(
            "user.generics.WrapperMethods.get_value_or_marker",
            new java.lang.String[] {"self"}, new java.lang.Object[] {this},
            "union[tv:T;user.generics.WrapperMarker]");
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
public class Box<T> {
    private final T value;
    private final baml_sdk.generics.Wrapper<T> wrapped;
    …
}
```

Free-function generics declare their type parameters on the Java method and
**infer the type args engine-side** — the arg carries the value, the descriptor
carries `tv:T`:

```java
// baml_sdk/generic_tests/Fns.java
public static <T> T identity(T x) {
    return (T) baml_bridge.BamlFfi.callSync(
        "user.generic_tests.identity", new java.lang.String[] {"x"}, new java.lang.Object[] {x}, "tv:T");
}
public static <A, B, C> baml_sdk.generic_tests.GenericTriple<A, B, C> make_triple(
        A a, java.util.List<B> b, java.util.Map<java.lang.String, C> c) { … }
```

A static factory that would collide with the Java `new` keyword is escaped:

```java
// baml_sdk/generic_tests/GenericBox.java  —  BAML static `new`
public static <T, V> baml_sdk.generic_tests.GenericBox<V> new$(V value) {
    return (baml_sdk.generic_tests.GenericBox<V>) baml_bridge.BamlFfi.callSync(
        "user.generic_tests.GenericBox.new", new java.lang.String[] {"value"}, new java.lang.Object[] {value},
        "user.generic_tests.GenericBox");
}
```

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

**NOT YET IMPLEMENTED IN JAVA (emitter surface).** — The generated methods above
are **inference-only**: no explicit-binding overload is emitted, and the
`class_type_params` / `type_params` metadata has no Java analog on the wire yet
(implicit-generics decode is green — 6/6 in type_shapes).

- **Runtime substrate: LANDED** overnight (commit `3991c4fd4`). The value-level
  type tokens exist in the runtime and pass value-equality tests:
  `baml_bridge/BamlType.java` (`INT`/`STRING`/`BOOL`/`FLOAT`, `of(Class)`,
  `of(Class, BamlType...)`, `toWireTy`/`fromWireTy`) and `baml_bridge/BamlTypes.java`
  (`BamlTypes.of("T", BamlType.INT).and(…)`, an ordered named bag).
- **Decided design (D3, 2026-07-17):** the call-site surface is a **named bag**
  `BamlTypes.of("T", BamlType.INT)` passed as a **trailing overload** (1:1 with
  the wire's named `BamlTyArg` bindings; partial binding allowed); token grammar
  is minimal-as-tested.
- **Still OPEN (Antonio):** readback naming (`bamlTypeArgs()` + `Fns$`-style
  collision escape vs always-`$`-named); and the trailing-overload matrix
  (req→opts→types→ctx, worst-case 16 methods) vs a fluent builder. The minimal
  grammar caps readback: a reified arg that is a list/map/union/optional/literal
  produces no side-table entry (`BamlType.fromWireTy` returns `null` out of
  grammar, `BamlType.java:172-206`) — decide widen-grammar vs document graceful
  degradation.
- Confirmed absent in generated output: `grep -rl BamlTypes` matches only the
  **test** files, never `generated/baml_sdk/**`; `GenericBox.java` has **no**
  reified `of(BamlType, value)` factory.

## Cross-Namespace References

Derived from `ns_symbol_collisions/ns_lorem/uses.baml`.

Cross-package field types are **fully-qualified references** — Java allows an FQN
in any type position, so no import machinery is needed and the namespace boundary
is preserved by the package path itself:

```java
// baml_sdk/symbol_collisions/lorem/Ipsum.java
package baml_sdk.symbol_collisions.lorem;

public class Ipsum {
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

**`baml.llm`** is a full generated namespace in Java too, mirroring Python:
`Client`, `PrimitiveClient`, `Context`, `RetryPolicy`, `StreamAccumulator`,
`ClientType` (enum), provider option classes, and a `baml/llm/Fns.java` that
emits `render_prompt`, `build_request`, `build_request_stream`,
`render_prompt_values`, etc. as ordinary `Fns` bindings (real quoted signatures):

```java
// baml_sdk/baml/llm/Fns.java (excerpt)
public static baml_sdk.baml.http.Request build_request( … ) { … }
public static baml_sdk.baml.llm.PromptAst render_prompt( … ) { … }
```

The runtime-owned `Stream` is re-exported: `baml.llm.Stream` resolves to
`baml_bridge.BamlStream` (`translate_ty.rs:180-186`).

> ⚠ **Deviation from Python:** Python re-exports handle-backed types with import
> aliases (`from baml_bridge.baml_py import BamlPdf as Pdf`; `from baml_bridge
> import BamlStream as Stream`). Java has no re-export shim — `translate_ty`
> resolves the type name **directly** to the runtime class
> (`baml_bridge.BamlStream`, `baml_sdk.baml.media.Image`). `translate_ty.rs:179-193`.

**Streaming calls / `Stream`**: **NOT YET IMPLEMENTED IN JAVA (runtime).** —
`baml_bridge/BamlStream.java` is a **compile-only stub**: every method throws
`UnsupportedOperationException` ("the streaming capability is not implemented
yet"). The generated companion classes (`Stream$stream`, `StreamAccumulator`)
and the type-map entries exist so the surface compiles, but no streaming call is
wired. *Open decision:* the target `BamlStream<TPartial, TFinal>` shape (see
`ref-java-state-of-completeness.md`, "Stream" row). Note: `final`/`final_async`
escape to `get_final`/`get_final_async` (Java reserved word).

**`build_request`** binding **is** emitted (quoted above), mirroring Python; the
task tracks the deeper `$build_request` host-request round-trip parity as a
later capability, so treat the end-to-end `$build_request` runtime behavior as
**NOT YET verified**, though the codegen surface is present.

## Stream Companion Types

Java keeps the compiler-produced `$stream` companion as an **in-package class
with the `$stream` name kept verbatim** — every optional-widened field boxes:

```java
// baml_sdk/primitives/Primitives$stream.java
public class Primitives$stream {
    private final java.lang.Long int_field;       // int   → boxed Long   (partial ⇒ nullable)
    private final java.lang.Double float_field;    // float → boxed Double
    private final java.lang.String string_field;
    private final java.lang.Boolean bool_field;
    private final java.lang.Void null_field;
    private final byte[] uint8array_field;
    …
}
```

The type map routes both the base and the companion FQN to their in-package Java
classes (from `Baml.java`):

```java
registerClass("user.primitives.Primitives",        "baml_sdk.primitives.Primitives",        …);
registerClass("user.primitives.Primitives$stream", "baml_sdk.primitives.Primitives$stream", …);
```

Generic and recursive companions follow the same in-package convention
(`generics/WrapperMethods$stream.java`, `aliases/RecList$stream.java`). As in
Python, codegen consumes the compiler-produced `$stream` class shape as a regular
class; it does not derive a `Partial[T]` transformation at Java codegen time.

> ⚠ **Deviation from Python:** Python puts companions in a **parallel
> `stream_types/<ns>` package** and names the class the base name (because `$` is
> not a Python identifier); the type map maps `…$stream` FQN → `stream_types`
> module. Java keeps `<Name>$stream` **beside its base type** (no parallel tree).
> `routing.rs:19-23,141-158` (routing ignores the `$stream` suffix). *Note (GAP B,
> handoff):* the ported `TestStreams` tests were written to Python's
> `stream_types.*` layout; reconciling them is an open call (rewrite tests to
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
public static java.util.List<java.lang.Long> optional_args_probe(long arg0) {
    return (java.util.List<java.lang.Long>) baml_bridge.BamlFfi.callSync(
        "user.optional_args_probe", new java.lang.String[] {"arg0"}, new java.lang.Object[] {arg0}, "list<int>");
}

public static java.util.List<java.lang.Long> optional_args_probe(
        long arg0, java.util.function.Consumer<optional_args_probe$Opts> $cfg) {
    optional_args_probe$Opts $opts = new optional_args_probe$Opts();
    $cfg.accept($opts);
    return (java.util.List<java.lang.Long>) baml_bridge.BamlFfi.callSync("user.optional_args_probe",
        $opts.$names(new java.lang.String[] {"arg0"}), $opts.$args(new java.lang.Object[] {arg0}), "list<int>");
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
public class Greeter {
    private final java.lang.String name;
    public Greeter(java.lang.String name) { this.name = name; }
    public java.lang.String name() { return this.name; }

    public static baml_sdk.methods_on_classes.Greeter create(java.lang.String name) {
        return (baml_sdk.methods_on_classes.Greeter) baml_bridge.BamlFfi.callSync(
            "user.methods_on_classes.Greeter.create",
            new java.lang.String[] {"name"}, new java.lang.Object[] {name}, "user.methods_on_classes.Greeter");
    }
    @SuppressWarnings("unchecked")
    public static java.util.concurrent.CompletableFuture<baml_sdk.methods_on_classes.Greeter> create_async(java.lang.String name) { … }

    public java.lang.String who() {                       // instance method: receiver prepended
        return (java.lang.String) baml_bridge.BamlFfi.callSync(
            "user.methods_on_classes.Greeter.who",
            new java.lang.String[] {"self"}, new java.lang.Object[] {this}, "string");
    }
    public java.lang.String greet(java.lang.String greeting) {
        return (java.lang.String) baml_bridge.BamlFfi.callSync(
            "user.methods_on_classes.Greeter.greet",
            new java.lang.String[] {"self", "greeting"}, new java.lang.Object[] {this, greeting}, "string");
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
    return (java.lang.String) baml_bridge.BamlFfi.callSync(
        "user.raises_test.LoadDoc", new java.lang.String[] {"path"}, new java.lang.Object[] {path}, "string");
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
// baml_sdk/host_callable_tests/Fns.java
public static java.lang.String call_with_callback(
        java.util.function.Function<java.lang.Long, java.lang.String> callback, long x) {
    return (java.lang.String) baml_bridge.BamlFfi.callSync(
        "user.host_callable_tests.call_with_callback",
        new java.lang.String[] {"callback", "x"}, new java.lang.Object[] {callback, x}, "string");
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
> shapes. A callable with **optional params or arity > 2** currently falls back
> to `java.lang.Object callback` (see the `call_callback_with_optional_args_*`
> bindings, `Fns.java:96-148`). `translate_ty.rs:403-435`.

**Runtime host-callable dispatch (the engine calling back *into* the host):**
**NOT YET IMPLEMENTED IN JAVA (runtime).** — The param types compile, but nothing
Java-side dispatches a callback: `bridge_java/src/lib.rs` never registers a host
dispatch/release callback, `BamlFfi` has no `hostDispatch`/`hostRelease`/
`registerHostCallable`/`nativeCompleteHostCall`, `ProtoWriter` rejects a
`Function` arg as "arbitrary object", and `ProtoReader` has no `BamlToHostCall`
decode.

Recommended design (decisions doc §4, slices 4a–4e — still OPEN, awaiting the owner):

- **Registry (A1):** a Java-side `ConcurrentHashMap<Long,Object>` in `BamlFfi`
  holds callables and opaque throwables; Rust stays a pure router. Gives
  `assertSame` identity for free (objects never leave the JVM).
- **Executor (B1):** a dedicated cached daemon `ExecutorService` runs user
  callables off the engine's tokio workers ("return promptly").
- **Async detection (C1):** detect a returned `CompletableFuture` at the value
  level and `.whenComplete` instead of encoding inline (no typed async surface
  yet — the two "async" ported tests actually assert the sync contract).
- **Exception identity (D):** native exc → opaque `HostCallable` handle
  round-trips the same `Throwable`; `class_name` = `getSimpleName()`.
- **`IntOptCallback` (E1):** optional/high-arity callables need a generated
  `@FunctionalInterface` + nested non-null `Opts` bag (nullable fields) — Java
  has no structural anonymous `$opts?: {…}` type. *Open:* signature-derived
  naming.
- **`BamlError(Object value)` ctor (F):** the throw-direction 1-arg ctor (Java
  today has only the 3-arg decode ctor, `BamlError.java:26`).

The generated `Person` / `ValidationError` value classes already exist
(`host_callable_tests/Person.java`, `ValidationError.java`).

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
