# BEP-044: Interfaces

## Summary

BAML has `class` (data shape) and `enum` (closed value set) but no way to say "this value has the shape `name: string` and the operation `speak() -> string`, no matter what concrete type produced it." This BEP introduces `interface` — a contract over fields and methods that classes explicitly implement via a scoped `implements` block inside the class body.

```baml
// ── 1. Declare an interface ──────────────────────────────────────────────

interface Animal {
  name: string
  age: int

  function speak(self) -> string
}

// ── 2. Implement it inside a class body ──────────────────────────────────
// Fields live on the class. The implements block links them by name
// (auto-matched when names align) and provides method bodies.

class Dog {
  breed: string
  name: string
  age: int

  implements Animal {
    // name and age auto-linked (same name & type as interface fields)
    function speak(self) -> string {
      return "Woof! My name is " + self.name
    }
  }
}

class Cat {
  indoor: bool
  name: string
  age: int

  implements Animal {
    function speak(self) -> string {
      return "Meow."
    }
  }
}

// ── 3. Construct — all fields use bare class-level names ─────────────────

let d = Dog { breed: "Lab", name: "Rex", age: 3 }

function describe(a: Animal) -> string {
  return a.name + " says " + a.speak()
}

// ── 4. Implement an interface FOR a type outside the class body ──────────
// Use `implements I for T { ... }` at the top level. This works for
// primitives, types you don't own, and types declared elsewhere.

interface ToJson {
  function to_json(self) -> json
}

implements ToJson for int {
  function to_json(self) -> json { return json.of(self) }
}

5.to_json()    // works — int now satisfies ToJson

// ── 5. Upcast to disambiguate generic interface instantiations ───────────

interface Container<E> {
  function add(self, item: E) -> null
  function size(self) -> int
}

function fill_ints<T extends Container<int>>(c: T, n: int) -> int {
  // If T also implements Container<float>, upcast to pick the right vtable.
  for (let i in 0..n) { c.as<Container<int>>.add(i) }
  return c.as<Container<int>>.size()
}
```

The design follows three guiding principles:

1. **TypeScript-shaped surface syntax, Rust-shaped scoping.** `interface Foo { ... }` is familiar to TS/Python/Java users. The `implements` block inside the class body is inspired by Rust's `impl Trait for Type`, but nested — each interface gets its own scope, making disambiguation natural.
2. **Default implementations.** Interfaces can provide method bodies that implementing classes inherit. Overrides are opt-in. The scoped `implements` block eliminates diamond ambiguity — each block independently resolves its interface's method chain.
3. **Nominal conformance.** `implements` is required, never inferred. A shape-matching class without `implements` does not conform.

No sealed interfaces in this version — that is deferred to a follow-up BEP.

## Motivation

Today, every reusable abstraction in BAML is either a concrete `class` or a closed `union` of concrete classes. That forces patterns that don't scale.

### Use case 1: Tool/agent dispatch

```baml
interface Tool {
  function call(self) -> string
}

class WebSearch {
  query: string

  implements Tool {
    function call(self) -> string { web.search(self.query) }
  }
}

class Calculator {
  expr: string

  implements Tool {
    function call(self) -> string { math.eval(self.expr) }
  }
}

function RunTool(t: Tool) -> string { return t.call() }
```

Adding a fourth tool doesn't touch `RunTool`.

### Use case 2: Cross-cutting shape with defaults

```baml
interface Timestamped {
  id: string
  created_at: datetime
  updated_at: datetime

  function is_stale(self, threshold: duration) -> bool {
    return now() - self.updated_at > threshold
  }
}

function MostRecent<T extends Timestamped>(items: T[]) -> T {
  // can sort by updated_at; can call is_stale() — default impl available
}
```

Implementors get `is_stale()` for free by declaring the three fields on the class and writing `implements Timestamped {}` (no method body needed — the default is inherited).

### Use case 3: Mockable / swappable boundaries

```baml
interface VectorStore {
  function search(self, query: string, k: int) -> Doc[]
  function upsert(self, docs: Doc[]) -> null
}

class PineconeStore {
  api_key: string
  implements VectorStore {
    function search(self, query: string, k: int) -> Doc[] { ... }
    function upsert(self, docs: Doc[]) -> null { ... }
  }
}

class InMemoryStore {
  implements VectorStore {
    function search(self, query: string, k: int) -> Doc[] { ... }
    function upsert(self, docs: Doc[]) -> null { ... }
  }
}
```

### Use case 4: Heterogeneous collections

```baml
interface Plugin {
  function init(self) -> null
}

let plugins: Plugin[] = [
  LoggingPlugin { ... },
  AuthPlugin { ... },
  RateLimitPlugin { ... },
]

for (let p in plugins) {
  p.init()
}
```

## Prior Art

| Language   | Structural? | Multiple? | Default methods? | Impl scoping |
|------------|-------------|-----------|------------------|--------------|
| TypeScript | Yes         | Yes       | No               | Inline (class body) |
| Go         | Yes         | Yes       | No               | N/A (structural) |
| Java       | Nominal     | Yes       | Yes (since 8)    | Inline (class body) |
| C#         | Nominal     | Yes       | Yes (since 8)    | Inline + explicit interface impl |
| Kotlin     | Nominal     | Yes       | Yes              | Inline (class body) |
| Swift      | Nominal     | Yes       | Yes (extensions) | Extensions (separate block) |
| Rust       | Nominal     | Yes       | Yes              | Separate `impl` blocks |

**Rust's `impl` blocks** are the closest prior art to our design. Rust separates the struct definition from trait implementations:

```rust
struct Dog { breed: String }

impl Animal for Dog {
    fn speak(&self) -> String { "Woof!".into() }
}
```

BAML nests the `impl` block inside the class for cohesion — all of a class's behavior is visible in one place — while preserving the per-interface scoping that makes Rust's disambiguation clean.

**C#'s explicit interface implementation** is the other close precedent. C# allows:

```csharp
class Product : IHasUuid, IHasSku {
    string IHasUuid.Id() => uuid;     // called via IHasUuid reference
    string IHasSku.Id() => sku;       // called via IHasSku reference
}
```

Our `implements` block achieves the same disambiguation without special syntax on each method — the block itself scopes everything inside it.

**Go's approach to field conflicts in embedding** (outer declaration wins; two embedded structs at the same depth with the same method require explicit disambiguation) informs our field conflict rules.

## Proposed Design

### Interface Declaration

```baml
interface InterfaceName {
  // Field signatures
  field_name: Type

  // Method signatures (no body = required, must be implemented)
  function method_name(self, param: Type) -> Return

  // Default methods (body provided, implementors inherit unless they override)
  function with_default(self, param: Type) -> Return {
    // default implementation
  }
}
```

- **`interface` keyword** matches TS exactly.
- Fields are signature-only — no default values on interface fields.
- Methods without a body are **required** — the `implements` block must provide them.
- Methods with a body are **defaults** — the `implements` block inherits them but can override.

### Method Signature Syntax

Method signatures in `interface` and `implements` blocks use an explicit `self` parameter as the first argument:

```baml
interface Animal {
  function speak(self) -> string
  function rename(self, new_name: string) -> null
}
```

This matches Rust/Python conventions (explicit `self`) rather than Java/TypeScript (implicit `this`). `self` carries no type annotation; the compiler types it as the enclosing interface inside default bodies and as the enclosing class inside `implements` blocks (with `Self`-type substitution where relevant — see [Self types](#self-types) below).

Methods declared *without* a `self` parameter are **reserved for future static methods** — they would belong to the interface or class itself rather than to instances, and would be invoked as `Interface.method()` or `Class.method()`. Static-method inheritance through `implements` is out of scope for v1 but is the motivation for requiring explicit `self` today: once static methods land, the parser already distinguishes the two.

The examples below sometimes elide `self` when not load-bearing to the rule being illustrated; in actual BAML syntax every instance-method signature includes it.

### The `implements` Block

The `implements` block is nested inside the class body. Each interface gets its own block.

**Keyword spelling.** Both `implement` and `implements` are accepted as keywords; the plural is the canonical form (matches TS / Java / Kotlin) and the singular is an accepted alias (reads naturally in `implement I for T { ... }`). Mix them freely — they parse identically. Examples in this document use `implements` consistently; reach for the singular when it reads better.

```baml
// Identical:
class Dog { implements Animal { ... } }
class Dog { implement  Animal { ... } }

implements ToJson for int { ... }
implement  ToJson for int { ... }
```

```baml
class MyClass {
  // Class's own fields
  x: int

  // Class's own methods (not part of any interface)
  function helper(self) -> string {
    return "I'm a helper"
  }

  // Interface implementation — scoped block
  implements SomeInterface {
    function required_method(self) -> int {
      return self.x * 2
    }
    // default methods inherited automatically if not listed here
  }

  implements AnotherInterface {
    function other_method(self) -> string {
      return self.helper()
    }
  }
}
```

**Key rules:**

1. Each `implements` block names exactly one interface.
2. The block must provide bodies for all **required** methods (those without defaults in the interface).
3. The block may **override** any default method by re-declaring it with a new body.
4. The block may be **empty** `implements Foo {}` if all required methods have defaults and all interface fields are auto-linked by name (see "Interface Fields" below).
5. Interface fields are **not injected** into the class. The class must declare them at its own top level. The `implements` block verifies the contract and can map names with `as` when they differ (see "Interface Fields" below).
6. A class can have any number of `implements` blocks.

### `self` Access

Inside an `implements` block, `self` refers to the class instance. All fields and methods are **flattened** — `self.field` and `self.method()` work directly for anything unambiguous:

- The class's fields: `self.name`, `self.x`
- The class's own methods: `self.helper()`
- Methods from other interface implementations: `self.greet()` if unambiguous, `self.as<Greeter>.greet()` if ambiguous

The same disambiguation rules from "Method Disambiguation" apply to `self` — if two interfaces define the same method name, you must upcast with `self.as<InterfaceName>.method()`. Otherwise, the direct call works.

```baml
interface Greeter {
  function greet(self) -> string
}

interface Farewell {
  function bye(self) -> string
}

class Polite {
  name: string

  implements Greeter {
    function greet(self) -> string {
      return "Hello, I'm " + self.name  // flat — name is unambiguous
    }
  }

  implements Farewell {
    function bye(self) -> string {
      // greet() is unambiguous (only Greeter has it) — direct call works
      return self.greet() + " — and goodbye!"
    }
  }
}
```

### Default Implementations

Interfaces can provide method bodies. Implementing classes inherit them unless they override.

```baml
interface Printable {
  name: string

  function display(self) -> string {
    return "[" + self.name + "]"
  }

  function verbose(self) -> string {
    return "Printable(" + self.display() + ")"
  }
}

class User {
  name: string
  email: string

  implements Printable {
    // name auto-linked by name match
    // display() inherited — returns "[<name>]"

    // Override verbose() only
    function verbose(self) -> string {
      return "User(" + self.name + ", " + self.email + ")"
    }
  }
}

class Item {
  name: string

  implements Printable {
    // name auto-linked; both display() and verbose() inherited
  }
}
```

#### Calling the default from an override

Use `default.method_name()` inside an `implements` block to call the interface's default implementation:

```baml
interface Logger {
  function log(self, msg: string) -> string {
    return "[LOG] " + msg
  }
}

class TimestampLogger {
  implements Logger {
    function log(self, msg: string) -> string {
      return now().to_string() + " " + default.log(msg)
    }
  }
}
```

`default` is only available inside an `implements` block and refers to the interface's own default implementation, not to any parent class.

**Scoping rules:**

- **Outside an `implements` block** — `default` is an unresolved name. Referencing it from a free function, class-own method, or top-level expression is a compile error.
- **Across lambdas** — `default` does *not* capture into lambda bodies. A lambda nested inside an override body sees `default` as an unresolved name. This matches BAML's general rule that magic identifiers don't cross closure boundaries; if needed, bind the result outside the lambda first.
- **Shadowing** — a local binding named `default` (e.g. `let default: string = ...`) wins over the magic identifier inside its scope. The keyword resolution only kicks in when no lexical `default` is in scope. This makes `default` discoverable but not reserved.
- **Repeated calls in one body** — `default.method(...)` may be called any number of times within one override body; each call is a static dispatch to the interface's default body and does not recurse through the override.
- **Required methods** — `default.method()` on a method that has no default body in the interface is a compile error: there is nothing to call.

### Interface Fields

Fields live on the **class**, not inside the `implements` block. The class declares all fields at its own top level; the `implements` block is a *contract assertion* that verifies the class has fields matching the interface's requirements. When an interface field name matches a class field name (and the types are identical), the link is automatic. When names differ — typically to resolve a conflict between two interfaces — the `implements` block uses `name as class_field` syntax to wire the interface's field to the class's field explicitly.

A consequence: **an interface that declares any fields can only be implemented inside a class body.** The out-of-body form (`implements I for T` at the top level — see §"Out-of-Body `implements`") cannot contribute fields, because the receiving type's data shape is already fixed at its declaration site. Field-bearing interfaces are therefore in-body-only; field-free interfaces work either way.

```baml
interface Config {
  host: string
  port: int
}

class Server {
  max_connections: int       // Class's own field
  host: string               // Satisfies Config.host (auto-linked by name)
  port: int                  // Satisfies Config.port (auto-linked by name)

  implements Config {}       // Empty — all fields match by name and type
}

// Construction: all fields use bare class-level names.
let s = Server {
  max_connections: 100,
  host: "localhost",
  port: 8080,
}

// Access: always use bare class-level names.
s.max_connections        // 100
s.host                   // "localhost"
s.port                   // 8080
```

Through an interface-typed variable, the interface's field names are used:

```baml
let c: Config = s
c.host                   // "localhost" — uses Config's field name
c.port                   // 8080
```

This is the same shape as TypeScript: `implements A` in TS doesn't inject A's fields; the class must declare them itself. The `implements` block just verifies the contract.

**Defaults on class fields satisfying interface contracts:**

The class can attach a default value to the field, making it optional at construction:

```baml
class DefaultServer {
  host: string = "localhost"
  port: int = 8080

  implements Config {}
}

let ds = DefaultServer {}    // OK — both fields have defaults
let ds2 = DefaultServer { host: "prod.example.com" }  // override one
```

#### The `as` linking syntax

When an interface field's name doesn't match any class field — because the class uses a different name, or because two interfaces both want a field called `name` — the `implements` block maps the interface field to the class field with `interface_field as class_field`:

```baml
interface Named   { name: string }
interface Labeled { name: string }

class Item {
  named_name: string
  labeled_name: string

  implements Named   { name as named_name }
  implements Labeled { name as labeled_name }
}

let i = Item { named_name: "widget", labeled_name: "WIDGET-001" }
i.named_name       // "widget"
i.labeled_name     // "WIDGET-001"
```

Through interface-typed variables, the interface field name resolves to the linked class field:

```baml
let n: Named = i
n.name             // "widget" — resolves through the `name as named_name` link

let l: Labeled = i
l.name             // "WIDGET-001" — resolves through the `name as labeled_name` link
```

If both interfaces have the same field name and same type, and you *want* them to share a single class field, just declare one field and leave both `implements` blocks empty:

```baml
class SharedItem {
  name: string

  implements Named   {}    // name auto-linked
  implements Labeled {}    // name auto-linked (same field)
}

let si = SharedItem { name: "widget" }
```

The author chooses per-case whether conflicting interface field names share one class field or map to separate ones.

#### Field Conflict Rules

**Rule 1 — Type match required.** Every class field linked to an interface field (whether by auto-matching or explicit `as`) must have *exactly* the same type — invariant, no subtyping permitted. A mismatch is a compile error (E0116):

```baml
interface Config { port: int }

class Server {
  port: string             // ERROR (E0116): class field `port` has type `string`
                           // but Config requires `port: int`. Types must match exactly.
  implements Config {}
}
```

**Rule 2 — Missing field is an error.** Every field declared on the interface must have a corresponding class field (by name match or explicit `as` link). A missing field is a compile error (E0113):

```baml
interface Config { host: string, port: int }

class Server {
  host: string

  implements Config {}
  // ERROR (E0113): class `Server` is missing field `port: int`
  // required by interface `Config`. Add `port: int` to the class
  // or link an existing field with `port as <class_field>`.
}
```

**Rule 3 — Invariant types.** Field types are matched *invariantly* — the class field must have the exact same type expression as the interface's. Even a subtype is rejected:

```baml
interface AnimalNode {
  parent: AnimalNode?
}

class Dog {
  parent: Dog?             // ERROR (E0116): expected `parent: AnimalNode?`,
                           // found `parent: Dog?`. Field types are invariant.
  implements AnimalNode {}
}
```

The class must spell the field type exactly as the interface declares it:

```baml
class Dog {
  parent: AnimalNode?      // OK — exact match

  implements AnimalNode {}
}
```

**Why not allow subtypes?** Subtype field declarations are unsound because fields are *read-write*. A writer sees the field at the interface's declared type and can store any value that satisfies it; a reader holding the concrete class expects the narrower type. If the field allows subtype declaration, a write through the interface can place a value that the class-side read cannot handle:

```baml
interface AnimalNode { parent: AnimalNode? }

class Dog {
  parent: Dog?          // hypothetically allowed as a subtype declaration
  implements AnimalNode {}
}

let d = Dog { parent: null }
let node: AnimalNode = d          // upcast — fine
node.parent = Cat { ... }        // writes a Cat — valid AnimalNode, not a Dog
d.parent                          // reads Dog? but gets a Cat — type violation
```

This is the classic covariant-write hazard (Java arrays have the same bug, caught at runtime with `ArrayStoreException`). BAML avoids it entirely: field types are invariant, period. We intentionally choose to be safer than TypeScript here, where structural subtyping silently permits this class of error.

Invariance matches the rest of BAML's nominal type system (variance is invariant for generic interfaces, too — see §"Generic Interfaces and Bounds") and keeps codegen straightforward: the runtime field slot has exactly the declared type, no upcast/downcast at access.

**Rule 4 — Different types across interfaces require explicit linking.** When two interfaces declare a field with the same name but different types, the class must have distinct fields for each and link them with `as`:

```baml
interface HasId    { id: string }
interface HasNumId { id: int }

class Thing {
  str_id: string
  num_id: int

  implements HasId    { id as str_id }
  implements HasNumId { id as num_id }
}

let t = Thing { str_id: "abc", num_id: 42 }
```

If the class tried to satisfy both with a single `id` field, the types would conflict and the compiler rejects it.

### Method Disambiguation

When a class implements multiple interfaces, method access follows these rules:

**Unambiguous — single interface or unique method name.** Direct call works:

```baml
// Given Dog implements Animal (with speak())
let d = Dog { breed: "Lab", name: "Rex", age: 3 }
d.speak()                    // OK — unambiguous, direct call works
d.as<Animal>.speak()         // Also OK — explicit upcast
```

**Ambiguous — two interfaces, same method name.** Must qualify:

```baml
interface A {
  function foo(self) -> int
}

interface B {
  function foo(self) -> int
}

class Hybrid {
  implements A {
    function foo(self) -> int { 1 }
  }
  implements B {
    function foo(self) -> int { 2 }
  }
}

let h = Hybrid {}
h.foo()              // ERROR: ambiguous — foo() is defined in both A and B.
                     // Use h.as<A>.foo() or h.as<B>.foo()
h.as<A>.foo()        // OK — returns 1
h.as<B>.foo()        // OK — returns 2
```

**Through an interface-typed variable — always unambiguous:**

```baml
let a: A = Hybrid {}
a.foo()  // OK — called through interface A, returns 1

let b: B = Hybrid {}
b.foo()  // OK — called through interface B, returns 2
```

This is the key insight: the interface type itself is the disambiguator. Holding a value at an interface type selects that interface's vtable.

**Different signatures — each in its own block, always qualified:**

```baml
interface Serializer {
  function encode(self) -> string
}

interface IntSerializer {
  function encode(self) -> int
}

class Data {
  implements Serializer {
    function encode(self) -> string { "json..." }
  }
  implements IntSerializer {
    function encode(self) -> int { 42 }
  }
}

let d = Data {}
d.encode()                          // ERROR: ambiguous
d.as<Serializer>.encode()           // OK — returns string
d.as<IntSerializer>.encode()        // OK — returns int
```

### Dispatch Sites

Any expression of interface type is a dispatch site. The vtable lookup happens on the value's runtime class, not on the static expression shape. All of the following dispatch dynamically:

```baml
let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }

// Local variable receiver
a.speak()

// Function-call result
get_animal().speak()

// Array / map index
zoo[0].speak()
directory["luna"].speak()
animals[i][j].speak()                  // nested indexing

// Field access chain (arbitrary depth)
wrapper.animal.speak()
root.a.b.c.d.e.speak()

// Pattern-bound local
match (a) { let d: Dog => d.speak(), _ => "?" }

// For-loop iteration variable
for (let x in animals) { x.speak() }

// Chained method calls
a.next().next().speak()
```

There is no syntactic restriction on receivers — interface dispatch fires wherever an interface-typed value reaches a method-call site. Implementors should not rely on any "static dispatch shortcut" for direct locals; the dispatch is value-driven everywhere.

### Interface Requirements (`requires`)

An interface may declare that it **requires** other interfaces. This is a contract on the *implementor* — every class that implements the child must also separately implement each required interface. The child interface itself does not absorb the parent's contract; it just states a precondition.

**Interfaces never `implement`.** An `interface` declaration may contain `requires` clauses but never an `implements` (or `implement`) clause — interfaces define contracts, they do not fulfill them. This keeps the three keywords cleanly disjoint:

| Keyword | Where it appears | Meaning |
|---|---|---|
| `requires` | On an `interface` declaration | This interface requires implementors to *also* implement the listed interfaces |
| `extends` | In a generic-parameter bound | The type parameter must satisfy this type expression |
| `implement` / `implements` | On a class, or at top level via `for` | This class (or type) fulfills the named interface's contract |

```baml
interface Named {
  name: string
}

interface Aged {
  age: int
}

interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string {
    return "I'm " + self.name + ", " + self.age.to_string() + ", " + self.occupation
  }
}

class Employee {
  salary: float
  name: string
  age: int
  occupation: string

  // Required impls — each named explicitly. Omitting either is a compile error.
  implements Named {}    // name auto-linked
  implements Aged  {}    // age auto-linked

  implements Person {}   // occupation auto-linked; introduce() inherited
}
```

A class that writes `implements Person {}` without also writing `implements Named {}` and `implements Aged {}` is rejected:

```baml
class Bad {
  implements Person {}
  // ERROR (E0125): class `Bad` implements `Person`, which requires
  // `Named` and `Aged`, but `Bad` does not implement them.
  // Add `implements Named {}` and `implements Aged {}`.
}
```

**Why `requires` and not `extends`.** `extends` (as in TS / Java / earlier drafts of this BEP) makes the *child interface* inherit the parent's signatures, so writing `implements Child` silently fulfilled the parent contract too. That bundles a lot into one declaration — implementor lists become hard to read, field injection happens transitively without local evidence, and diamond inheritance needs special rules. `requires` keeps every conformance local: each `implements I` on a class names exactly one interface and fulfills exactly that interface's contract. This matches Rust's supertrait model (`trait B: A` means "to implement B you must also implement A separately").

**Type-system behavior.** Values typed at a child interface still expose every required parent's methods and fields, because the type checker knows the requirement chain. `Person` carries `Named`'s and `Aged`'s fields through the registry, just like Rust's `dyn Pet` lets you call `dyn Animal`'s methods.

```baml
function greet(p: Person) -> string {
  return p.name + " (" + p.occupation + ")"  // p.name comes via the Named requirement
}
```

**Diamond requires — no ambiguity because of block scoping:**

```baml
interface A {
  function foo(self) -> string { "A" }
}

interface B requires A {
  function foo(self) -> string { "B" }
}

interface C requires A {
  function foo(self) -> string { "C" }
}

class D {
  implements A {}  // foo() inherits "A"
  implements B {}  // foo() inherits "B"
  implements C {}  // foo() inherits "C"
}

let d = D {}
d.foo()           // ERROR: ambiguous — three contributing interfaces
d.as<A>.foo()     // "A"
d.as<B>.foo()     // "B"
d.as<C>.foo()     // "C"
```

Each `implements` block resolves its own interface's method chain independently — the diamond can't produce a conflict because conformance is per-block, not transitive.

**Conflicting field requirements across the requires graph is a compile error on the child interface:**

```baml
interface X { id: string }
interface Y { id: int }

interface Z requires X, Y {}
// ERROR: interface `Z` requires both `X` and `Y`, which contribute conflicting
// types for field `id`: `string` from `X`, `int` from `Y`. No class can
// implement `Z`.
```

### Class Methods Outside Interfaces

A class can have methods that are not part of any interface:

```baml
class Server {
  host: string
  port: int

  // Class's own method
  function address(self) -> string {
    return self.host + ":" + self.port.to_string()
  }

  implements Configurable {
    function configure(self, opts: Options) -> null {
      // Can call class's own methods
      let addr = self.address()
      // ...
    }
  }
}
```

Class methods are always accessible as `instance.method()` — they never conflict with interface methods because interface methods are scoped to their `implements` block. If a class method has the same name as an interface method, the class method takes precedence on direct call; use `instance.as<InterfaceName>.method()` to reach the interface version.

### Out-of-Body `implements` (`implements I for T`)

An `implements` block may also be written at the top level, naming both the interface and the receiving type. This is the same form used inside the class body, with the receiver lifted out and reintroduced after `for`. The two forms produce the same HIR — same registry entries, same diagnostics, same codegen.

> **Critical restriction:** Out-of-body `implements` can only target **field-free interfaces**. The receiving type's data shape is already fixed at its declaration; an out-of-body block has no power to add fields to it. If interface `I` declares *any* fields, `implements I for T { ... }` at the top level is a compile error (E0123), regardless of what `T` is. Field-bearing interfaces must be implemented inside the class body so the class's fields can be linked.

```baml
interface ToJson {
  function to_json(self) -> json    // field-free — out-of-body is fine
}

// Implement ToJson for a primitive — no class body to put this in.
implements ToJson for int {
  function to_json(self) -> json { return json.of(self) }
}

implements ToJson for string {
  function to_json(self) -> json { return json.of(self) }
}

implements ToJson for bool {
  function to_json(self) -> json { return json.of(self) }
}

5.to_json()        // works for int
"hi".to_json()     // works for string
```

```baml
// COMPILE ERROR — `Named` declares a field, so out-of-body is forbidden.

interface Named { name: string }

implements Named for int {        // ERROR (E0123): out-of-body `implements`
  // ...                           // cannot target an interface that declares
}                                  // fields. Field-bearing interfaces must be
                                   // implemented inside the class body.
```

The receiver in `for T` may be any nameable type — a class declared in this file, a class in another module, or a primitive. The interface's methods become callable as instance methods on `T`. The "no fields" restriction means out-of-body `implements` is the right tool for **behavioral** mix-ins (serialization, hashing, comparison) and never for **data** mix-ins.

**Rules:**

1. **No field linking, ever.** An out-of-body `implements` block contributes methods only. It cannot link fields from the receiving type to interface fields. If interface `I` declares any fields, `implements I for T { ... }` outside the class body is unconditionally rejected (E0123) — there is no escape hatch where the class "already happens to have" a matching field. To make `T` satisfy a field-bearing interface, write the `implements` block inside `T`'s class body so the class's fields can be linked.

2. **One implementation per `(interface, type)` pair across the whole project.** Whether the `implements` block lives inside the class body or out at the top level, declaring it twice anywhere is a duplicate (E0114).

3. **Orphan rule — must own one side.** An out-of-body `implements I for T` is only allowed in the package that owns *either* `I` or `T`. Implementing a third-party interface for a third-party type is forbidden (E0126). This keeps the schema-of-implementors local to a package and prevents action-at-a-distance from sibling dependencies.

4. **Receiver-class scoping.** Inside the block, `self` is typed as `T`. The same `Self` substitution rules apply: `Self` in the signature refers to `T`. Out-of-body blocks have the same access to `default.method()`, `obj.as<Interface>.method()`, and every other in-body affordance.

5. **Aggregation.** Codegen treats the union of every in-body and out-of-body `implements` block for a class as that class's behavior surface. Python/TS emit a single class definition; Rust emits one `impl Trait for Type` block per interface.

6. **Discoverability.** The LSP surfaces out-of-body impls in hover, outline, and "find all implementations" results so a reader looking at `class Dog { ... }` can still see every interface `Dog` conforms to.

```baml
// Out-of-body impl on a class defined elsewhere in the project.

class Server { host: string, port: int }

implements ToJson for Server {
  function to_json(self) -> json {
    return json.of({ host: self.host, port: self.port })
  }
}
```

The class declaration stays focused on its own shape; cross-cutting concerns like serialization attach where they belong — next to the interface they fulfill.

### Converting Between Interface Types

A value held at one interface type is **not** directly assignable to another interface type, even when its concrete class implements both contracts:

```baml
interface Animal  { function speak(self) -> string }
interface Swimmer { function swim(self) -> string }

class Dog {
  implements Animal  { function speak(self) -> string { "Woof!" } }
  implements Swimmer { function swim(self) -> string  { "splash" } }
}

let a: Animal = Dog {}
let s: Swimmer = a       // ERROR: `Animal` is not assignable to `Swimmer`
```

To use the value at a different interface, narrow to the concrete class first via `match` (or `is`), then assign the narrowed binding to the target interface:

```baml
let a: Animal = Dog {}

match (a) {
  let d: Dog => {
    let s: Swimmer = d   // OK — d is Dog, Dog implements Swimmer
    s.swim()
  }
  _ => "not a swimmer"
}
```

Rationale: an interface-typed value carries only the interface's vtable; the concrete class is dynamic. Cross-interface conversion would require a runtime class check, and BAML makes that check explicit through pattern matching rather than hiding it inside an assignment. This also means the static type of every variable accurately predicts which methods are callable on it.

### Subtyping Rules

Related to interfaces (but not part of this BEP itself), we need to formalize our subtyping rules (and unify their implementation in the TIR). Subtypes represent a subset relationship.

Being a subset relationship, this means we can infallibly widen a subtype into a supertype. We allow implicit upcasting in these cases (and only these cases): `let a: MyInterface = Foo.new()` (concrete to non-concrete) and `let a: bigint = 123` (concrete to concrete).

**Concrete vs non-concrete types.** A *concrete* type is something like a class, a primitive, or an enum — it corresponds to some concrete memory layout and method implementations. *Non-concrete* types include unions, interfaces, and `unknown`, which must use dynamic dispatch to the instance's concrete type methods at runtime. Every instance/value has exactly one concrete type at runtime, even if the type of the variable binding is non-concrete. Non-concrete types still "exist" at runtime to be used via reflection APIs and patterns.

This distinction simplifies the question of what types can implement an interface: concrete types can implement an interface, while non-concrete types (unions, `unknown`, or other interfaces) may not, as they can't actually implement anything themselves. `never` is a concrete type (just with no members) so it *can* implement interfaces. However since there are never any instances of `never`, only static methods can ever be executed.

**Closed subsets (literals):**

- `(int.min_value() | ... | -2 | -1 | 0 | 1 | 2 | ... | int.max_value()) =: int`
- `(true | false) =: bool`
- `("" | "\x00" | "\x01" | ... | "all strings" | ...) =: string` (infinite size set)
- `(... | -2n | -1n | 0n | 1n | 2n | ...) =: bigint` (infinite size set)
- `(Enum.VariantA | Enum.VariantB | ...) =: Enum` (for each enum type)

**Other subtypes:**

- `never <: T` for all `T`
- `T <: unknown` for all `T`
- `T <: (T | ...)` for all `T`
  - Including unions: `(T0 | T1) <: (T0 | T1 | ...)`
  - Also including optionals: `T? =: T | null`
- `T =: T` for all `T`
- `A <: B` for interfaces where `A requires B`
- `C <: I` for concrete type `C` that implements interface `I`
- `((a0, a1, ..., ao0=..., ...) -> ar throws ae) <: ((b0, b1, ..., bo0=..., ...) -> br throws be)` if:
  - Same number of positional arguments where each `bN <: aN`
  - Each optional argument `boN` has a corresponding `aoN` of the same name where `boN <: aoN`
  - `ar <: br`
  - `ae <: be`
- `int <: bigint`

We currently treat all generics as invariant: `Foo<A>` has no subtyping relationship with `Foo<B>` even if `A <: B`. We can change this at a later date if we want.

Additionally, we currently have `int` as an effective subtype of `float`: however, this is incorrect as upcasting is fallible (there are 64 bits in `int` but effectively only 53 in the mantissa of `float` so not all `int` are representable as a `float`). We should instead require explicit fallible casting for conversion (we can also define fallible `int x float` operators which should remove most of the pain points).

### `Self` Types

`Self` (capital S) is a *type-level* alias meaning "the concrete type that implements this interface." It is distinct from the lowercase `self` parameter (which is a *value*, the method receiver). Use `Self` whenever a signature must preserve the concrete implementor's identity rather than collapsing to the interface. Note that `self` is always implicitly `self: Self`, so it counts as a `Self` usage.

**Comparison to other languages:** `Self` matches Rust's `Self` and Swift's `Self` (both type-level aliases for the implementor) and Python's `typing.Self` (PEP 673). TypeScript's `this`-as-a-type is the closest precedent that reuses an existing keyword; BAML uses a distinct identifier (`Self`) to avoid overloading lowercase `self`. Java/C#/Kotlin lack a direct equivalent and force F-bounded generics (`<T extends Cloneable<T>>`); we explicitly avoid that pattern.

`Self` can appear in three positions within a method signature:

1. **Arguments** — `arg1: Self`, `arg2: map<string, Self>`. Note that `self` is always implicitly `self: Self` so this also counts.
2. **Return type** — `-> Self`, `-> Self?`
3. **Error type** — `throws Self`, `throws Self | baml.errors.InvalidArgument`

**The core rule: methods with multiple `Self`s in their parameters can only be called on a concrete-typed receiver.** All `Self` checking is enforced at compile time — no runtime panics.

```baml
interface Foo {
  function bar(self) -> int
  function baz(self, other: Self) -> bool
}

class Lorem {
  implements Foo {
    function bar(self) -> int { return 42 }
    function baz(self, other: Self) -> bool { return true }
  }
}
```

Calling through an interface-typed vs concrete-typed receiver:

```baml
function example(a: Foo, b: Foo) -> bool {
  let c = a.bar()          // OK — only one `Self` (the receiver), so we can
                           // call on interface-typed `a` and safely
                           // disambiguate at runtime
  let _ = a.baz(b)         // COMPILE ERROR: non-concrete `Self` type —
                           // `baz` has two `Self` params
  if (a is Lorem && b is Lorem) {
    a.baz(b)               // OK — concrete `Lorem.Foo.baz` to dispatch
  } else {
    false
  }
}
```

**Return type behavior.** When a concrete type for `Self` is known for the parameters, the return type uses the same concrete `Self` type:

```baml
interface Foo {
  function bar(self) -> Self
  function baz(self, other: Self) -> Self
}

class Lorem {
  implements Foo {
    function bar(self) -> Self { return Lorem {} }
    function baz(self, other: Self) -> Self { return other }
  }
}

// All types are concrete — return types stay concrete.
function example(a: Lorem, b: Lorem) -> Lorem {
  let c: Lorem = a.bar()      // OK — concrete dispatch has concrete return
  let d: Lorem = a.baz(b)     // OK — concrete dispatch, args follow args rule
  c.baz(d)
}
```

When the receiver is interface-typed, the return type collapses to the interface type:

```baml
function example2(a: Foo, b: Foo) -> Foo {
  let c: Foo = a.bar()        // OK — cannot make guarantees about concrete types,
                               // so return is Foo (the interface type)
  let d: Foo = a.baz(b)       // COMPILE ERROR: because of args rule (two Self params)
  let e: Lorem = a.bar()      // COMPILE ERROR: implicit downcast to `Lorem`
  c.bar()
}
```

The error type (`throws`) behaves the same as the return type, since it is effectively part of the return type.

**Static methods on interfaces.** These rules extend naturally to static methods (future work). The rule generalizes to: calls to interface methods without a concrete type must have exactly one `Self` in the parameters.

```baml
interface AdditiveIdentity requires Add {
  // Zero `Self` in params — only callable with concrete `Self`
  function additive_identity() -> Self
}

class Int {
  implements AdditiveIdentity {
    function additive_identity() -> Self {
      0
    }
  }
}
```

**Default method bodies with `Self` return.** The earlier version of this BEP stated that methods with return type `Self` cannot have default implementations. This is probably fine for now, but eventually we may be able to use reflection on `Self` to produce type-safe general-case default implementations, with the understanding that `Self` represents an arbitrary concrete type implementing the interface, not the interface type itself:

```baml
interface MultAdd requires Mult, Add {
  function mad(self, m: Self, a: Self) -> Self {
    self.as<Mult>.mult(m).as<Add>.add(a)
  }
}
```

**`Self` outside an `interface` or class body** is a compile error. Free functions cannot reference `Self` because there is no enclosing type.

**Summary.** These rules follow the concrete vs non-concrete type distinction from §"Subtyping Rules": calling a method with a concrete type dispatches with a concrete `Self` (and thus can have any number of `Self`s in the parameters, as long as those `Self`s provably match the concrete `Self` type), whereas non-concrete types engage in dynamic dispatch and thus must have exactly one `Self` in the parameters.

### Generic Interfaces and Bounds

Interfaces can be parameterized:

```baml
interface Container<T> {
  function add(self, item: T) -> null
  function get(self, index: int) -> T?
  function size(self) -> int
}

class Stack<T> {
  items: T[] = []

  implements Container<T> {
    function add(self, item: T) -> null {
      self.items = self.items + [item]
    }
    function get(self, index: int) -> T? {
      return self.items[index]
    }
    function size(self) -> int {
      return self.items.length()
    }
  }
}
```

**Generic bounds with `extends`:**

A generic parameter can be constrained by an `extends` clause. The clause accepts a **type expression** — any type the surface grammar admits in type position. This includes interface names, concrete class names, primitive types, generic-interface instantiations, unions, optional (`T?`), arrays (`T[]`), and combinations thereof. The shape mirrors TypeScript's generic bounds.

```baml
// Interface bound
function first_by_name<T extends Named>(items: T[]) -> T? {
  return items.sort_by((a, b) => a.name.compare(b.name)).first()
}

// Primitive-union bound
function double<T extends int | float>(x: T) -> T {
  return x + x
}

// Generic-interface bound
function copy_into<T extends Container<int>>(c: T, xs: int[]) -> null {
  for (let x in xs) { c.add(x) }
}

// Union of interfaces — T satisfies either
function tagged<T extends Named | Labeled>(t: T) -> string {
  return match (t) {
    let n: Named   => n.name
    let l: Labeled => l.name
  }
}

// Compound expression
function head<T extends (int | string)[]>(xs: T) -> int | string {
  return xs[0]
}
```

The bound is checked statically at every call site: the compiler rejects `first_by_name<Rock>(...)` when `Rock` does not implement `Named`, rejects `double<bool>(...)` when `bool` is not in the union, etc. The keyword is distinct from `requires` (used on interfaces themselves) — `extends` is purely a generic-parameter constraint, the way TS/Java/Kotlin use it.

**Multi-instantiation of generic interfaces.**

A class may implement the same generic interface with different type arguments. Direct (unqualified) calls are ambiguous; the `.as<Interface>` upcast selects which instantiation's vtable to dispatch through:

```baml
interface Converter<T> {
  function convert(self) -> T
}

class MultiFormat {
  data: string

  implements Converter<int>   { function convert(self) -> int   { return 42 } }
  implements Converter<float> { function convert(self) -> float { return 42.5 } }
}

let m = MultiFormat { data: "42" }
m.convert()                             // ERROR (E0121): ambiguous —
                                        // Converter<int> vs Converter<float>
m.as<Converter<int>>.convert()          // 42
m.as<Converter<float>>.convert()        // 42.5
```

`.as<Interface>` is a **method-style upcast**: it picks the vtable to dispatch through at the call site. Because the class statically implements both instantiations, no runtime check is needed. The method-call syntax chains naturally — no parentheses needed.

**Implicit upcasting** also works when assigning to an interface-typed variable:

```baml
let ci: Converter<int> = m
ci.convert()                            // 42 — unambiguous through the variable type

let cf: Converter<float> = m
cf.convert()                            // 42.5
```

`.as<I>` is the universal disambiguation mechanism — it works for both generic and non-generic interfaces:

| Situation | Syntax |
|---|---|
| Two non-generic interfaces, same method name | `d.as<Serializer>.encode()` |
| Same generic interface, different type args | `m.as<Converter<int>>.convert()` |

**Upcasting to a non-generic interface** works too:

```baml
let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }

// Implicit upcast (assignment):
let a2: Animal = Dog { breed: "Poodle", name: "Buddy", age: 4 }

// Explicit upcast (method-style):
Dog { breed: "Lab", name: "Rex", age: 3 }.as<Animal>.speak()
```

**Downcasting** (from an interface type to a concrete class, or from one interface to another) always requires `match` / `is` — `.as<T>` is never used for downcasts.

**Variance** is invariant in v1. `Container<Dog>` and `Container<Animal>` are unrelated types even if `Dog` implements `Animal`. This matches Go and BEP-013's position.

### Interaction with `match` (BEP-015)

**Interface as a type pattern:**

```baml
function describe(val: Animal | string) -> string {
  match (val) {
    Animal { name } => "animal: " + name
    string => "raw: " + val
  }
}
```

**Narrowing from interface to concrete class:**

```baml
function details(a: Animal) -> string {
  match (a) {
    a: Dog => "Dog breed: " + a.breed
    a: Cat => "Cat indoor: " + a.indoor.to_string()
    _ => a.name + " (unknown species)"
  }
}
```

The `_` catch-all is **required** for open interfaces since new implementors can appear. (Sealed interfaces, when added later, would make exhaustive matching possible without `_`.)

### LLM Functions

Interfaces compose with LLM functions through existing union machinery. The compiler enumerates implementors at the call site and renders as `oneOf`:

```baml
function DetectAnimal(image: image) -> Animal {
  client GPT4o
  prompt #"Identify the animal. {{ ctx.output_format }}"#
}
// Renders as oneOf(Dog, Cat, ...) based on all classes implementing Animal
```

| Position | Behavior |
|---|---|
| Parameter (`a: Animal`) | Host passes a concrete value; serialized as that concrete class's shape |
| Return (`-> Animal`) | Implementors enumerated, rendered as `oneOf`, parsed structurally |
| Generic return (`<T extends Animal> -> T`) | Same enumeration for the candidate set of `T` |

Adding `class Robot { implements Animal { ... } }` anywhere in the project extends the `oneOf` schema for every LLM function returning `Animal`. This is the deliberate trade-off of open interfaces.

### Runtime Reflection (BEP-039)

`reflect.type_of<T>()` (BEP-039) already returns an opaque `type` value with `.to_string()` and `==`/`!=`. Interfaces extend this with three new operations on `type`:

```baml
// Existing — works today
let dog_t: type = reflect.type_of<Dog>()
let animal_t: type = reflect.type_of<Animal>()
dog_t.to_string()       // "Dog"
animal_t.to_string()    // "Animal"
dog_t == animal_t       // false — different types

// New — proposed by this BEP
dog_t.implements(animal_t)         // true — Dog implements Animal
animal_t.implemented_by(dog_t)     // true — same question, reverse direction
animal_t.implementors()            // [type-of(Dog), type-of(Cat), type-of(Duck)]
```

**`implements(t: type) -> bool`** — returns true if the receiver type implements interface `t`. Compile-time check; the answer is known statically.

**`implemented_by(t: type) -> bool`** — reverse of `implements`. `animal_t.implemented_by(dog_t)` is equivalent to `dog_t.implements(animal_t)`.

**`implementors() -> type[]`** — returns the list of concrete types that implement this interface, in declaration order. Only valid on interface types; calling on a non-interface type returns `[]`.

These compose with generic functions:

```baml
function is_animal<T>() -> bool {
  reflect.type_of<T>().implements(reflect.type_of<Animal>())
}

is_animal<Dog>()   // true
is_animal<int>()   // false
```

**Value-level reflection** (`reflect.type_of_value(x)` — obtain the concrete runtime type of a value held at an interface type) is deferred to BEP-039's follow-up. When it lands, it enables:

```baml
let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }
reflect.type_of_value(a) == reflect.type_of<Dog>()   // true (future)
```

### Codegen

| Language   | Interface projection | `implements` block projection |
|------------|---------------------|-------------------------------|
| TypeScript | `interface Animal { ... }` | Class body + `implements Animal` on declaration |
| Python     | Abstract `BaseModel` with `@abstractmethod` | Class subclasses the interface base |
| Rust       | `trait Animal { ... }` | `impl Animal for Dog { ... }` (separate block, Rust-native) |
| Go         | `type Animal interface { ... }` | Methods on struct (Go is structural) |
| C#         | `interface IAnimal { ... }` | Explicit interface implementation |

**Python:** Pydantic base class, not Protocol. Fields live on the class; `as` links are resolved at codegen time:

```python
class Animal(BaseModel):
    name: str
    age: int
    @abstractmethod
    def speak(self) -> str: ...
    
class Dog(Animal):
    breed: str
    def speak(self) -> str:
        return "Woof!"
```

When `as` linking is used (e.g., `name as foo_name`), the generated class has the class-level field name (`foo_name`), and interface-typed access maps through the link.

**Rust:** The `implements` block maps directly to Rust's separate `impl Trait for Type` blocks — the most natural projection. `as` links become getter methods in the trait impl.

**TypeScript:** The `implements` block collapses to TS's native `class Dog implements Animal` since TS doesn't scope methods per interface. Class-flat fields make this projection trivial.

## Corner Cases

This section catalogs every edge case the compiler must handle. The test suite below exercises each one.

### Fields

| Case | Rule | Result |
|---|---|---|
| Class field matching interface field by name and type | Auto-linked | OK — empty `implements` block suffices |
| Class field matching interface field by name, wrong type | Type mismatch | Compile error (E0116) |
| Class field linked via `as` with wrong type | Type mismatch | Compile error (E0116) |
| Interface field with no matching class field and no `as` link | Missing field | Compile error (E0113) |
| Two interfaces, same field name, same type, one class field | Shared auto-link | Both interfaces link to the same class field |
| Two interfaces, same field name, different types | Explicit `as` required | Each must link to a distinct class field via `as` |
| Field type is a subtype of interface field type | Invariant | Compile error (E0116) — exact type match required |
| Interface `requires` another with conflicting field types | Unsatisfiable contract | Compile error on the requirer interface |

### Methods

| Case | Rule | Result |
|---|---|---|
| Required method, not provided in `implements` | Missing impl | Compile error |
| Default method, not overridden | Inherited | Class gets default body |
| Default method, overridden | Override wins | Class uses override |
| Empty `implements {}`, all methods have defaults | Valid | All defaults inherited |
| Empty `implements {}`, some methods required | Missing impl | Compile error |
| Two interfaces, same method name, same signature | Ambiguous | `obj.foo()` errors; use `obj.as<I>.foo()` |
| Two interfaces, same method name, different signatures | Separate | Always requires `.as<I>` upcast |
| Single interface, call without qualification | Unambiguous | `obj.foo()` works |
| Call through interface-typed variable | Scoped | `(a: A).foo()` always unambiguous |
| Class method same name as interface method | Class wins on direct call | Use `obj.as<I>.method()` for interface version |

### Generics

| Case | Rule | Result |
|---|---|---|
| Generic interface with concrete type param | Monomorphized | `implements Container<int>` |
| Same generic interface, different type params | Allowed | Must disambiguate with `.as<I<T>>` or interface-typed variable |
| Generic bound `<T extends I>` | T must satisfy interface I | Compiler checks at every call site |
| Union bound `<T extends int \| string>` | T must be one of the listed types | Compiler checks at every call site |
| Intersection bound (`&` in bound) | Not supported in v1 | See Open Questions |

### Inheritance

| Case | Rule | Result |
|---|---|---|
| Interface `requires` another | Implementor must implement each separately | Compile error if any required `implements` is missing |
| Diamond `requires`, same method | No ambiguity | Each `implements` block resolves independently |
| Circular `requires` | Illegal | Compile error |

### Reflection

| Case | Rule | Result |
|---|---|---|
| `type_of<Interface>()` | Returns interface type value | `.to_string()` = `"Animal"` |
| `type_of<Class>()` | Returns class type value | `.to_string()` = `"Dog"` |
| `class_t.implements(iface_t)` | Nominal check | `true` if class has `implements` block |
| `iface_t.implemented_by(class_t)` | Reverse of above | Same result |
| `iface_t.implementors()` | List all implementing classes | `type[]` in declaration order |
| `implements` through `requires` chain | Direct only | Employee must `implements Named` separately; reflection sees both |
| `type_of<Container<int>>` vs `type_of<Container<string>>` | Different types | `!=` |
| `type_of` on non-interface `.implementors()` | No implementors | Returns `[]` |

## Test Suite

The following tests exercise every scenario above. They use BAML's test syntax (BEP-023).

### Setup: Interface and Class Declarations

```baml
// ── Interfaces ──

interface Animal {
  name: string
  age: int

  function speak(self) -> string
  function describe(self) -> string {
    return self.name + " (age " + self.age.to_string() + ")"
  }
}

interface Swimmer {
  function swim(self) -> string
  function speed(self) -> float
}

interface Named {
  name: string
}

interface Aged {
  age: int
}

interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string {
    return self.name + ", " + self.occupation
  }
}

interface HasId {
  id: string
}

interface HasNumId {
  id: int
}

interface Serializer {
  function encode(self) -> string
}

interface IntSerializer {
  function encode(self) -> int
}

interface Logger {
  function log(self, msg: string) -> string {
    return "[LOG] " + msg
  }
}

interface Closeable {
  function close(self) -> null
}

interface Container<T> {
  function add(self, item: T) -> null
  function get(self, index: int) -> T?
  function size(self) -> int { return 0 }  // default
}

interface Converter<T> {
  function convert(self) -> T
}

interface Greeter {
  function greet(self) -> string
}

interface Farewell {
  function bye(self) -> string
}

// For diamond testing
interface Base {
  function foo(self) -> string { "Base" }
}

interface Left requires Base {
  function foo(self) -> string { "Left" }
}

interface Right requires Base {
  function foo(self) -> string { "Right" }
}

// ── Classes ──

class Dog {
  breed: string
  name: string
  age: int

  implements Animal {
    function speak(self) -> string { return "Woof!" }
    // describe() inherited from Animal default
  }
}

class Cat {
  indoor: bool
  name: string
  age: int

  implements Animal {
    function speak(self) -> string { return "Meow." }
    function describe(self) -> string {
      let loc = match (self.indoor) { true => "indoor", false => "outdoor" }
      return self.name + " the " + loc + " cat"
    }
  }
}

class Duck {
  color: string
  name: string
  age: int

  implements Animal {
    function speak(self) -> string { return "Quack!" }
  }

  implements Swimmer {
    function swim(self) -> string { return self.name + " swims gracefully" }
    function speed(self) -> float { return 3.5 }
  }
}

class Employee {
  salary: float
  name: string
  age: int
  occupation: string

  // Person requires Named and Aged, so we implement each one explicitly.
  implements Named  {}    // name auto-linked
  implements Aged   {}    // age auto-linked
  implements Person {}    // occupation auto-linked; introduce() inherited
}

class Hybrid {
  implements Serializer {
    function encode(self) -> string { return "json:{}" }
  }
  implements IntSerializer {
    function encode(self) -> int { return 42 }
  }
}

class TimestampLogger {
  prefix: string = "TS"

  implements Logger {
    function log(self, msg: string) -> string {
      return self.prefix + " " + default.log(msg)
    }
  }
}

class Diamond {
  // Left and Right both require Base — every required interface must be
  // implemented explicitly.
  implements Base  {}
  implements Left  {}
  implements Right {}
}

class SimpleStack<T> {
  items: T[] = []

  implements Container<T> {
    function add(self, item: T) -> null {
      self.items = self.items + [item]
    }
    function get(self, index: int) -> T? {
      return self.items[index]
    }
    function size(self) -> int {
      return self.items.length()
    }
  }
}

// Class with its own method + interface
class Server {
  max_connections: int = 100
  host: string
  port: int

  function address(self) -> string {
    return self.host + ":" + self.port.to_string()
  }

  implements Config {}
}

interface Config {
  host: string
  port: int
}

// Class with field defaults satisfying Config
class DefaultServer {
  host: string = "localhost"
  port: int = 8080

  implements Config {}
}

// For Printable default-method tests (§Default Implementations)
class Polite {
  name: string

  implements Greeter {
    function greet(self) -> string {
      return "Hello, I'm " + self.name
    }
  }

  implements Farewell {
    function bye(self) -> string {
      return self.greet() + " — and goodbye!"
    }
  }
}

interface Printable {
  name: string

  function display(self) -> string {
    return "[" + self.name + "]"
  }

  function verbose(self) -> string {
    return "Printable(" + self.display() + ")"
  }
}

class Item {
  name: string
  implements Printable {}
}

class User {
  name: string
  email: string

  implements Printable {
    function verbose(self) -> string {
      return "User(" + self.name + ", " + self.email + ")"
    }
  }
}

// For `as` field linking test
interface Labeled { name: string }

class DualNamed {
  real_name: string
  label_name: string

  implements Named   { name as real_name }
  implements Labeled { name as label_name }
}

// For multi-instantiation / upcast test
class MultiFormat {
  data: string

  implements Converter<int>   { function convert(self) -> int   { return 42 } }
  implements Converter<float> { function convert(self) -> float { return 42.5 } }
}

// ── Error cases (these should NOT compile) ──

// COMPILE ERROR: field type mismatch
// class BadFieldType {
//   host: string
//   port: string   // Config requires int — type mismatch
//   implements Config {}
// }

// COMPILE ERROR: missing field for two interfaces with conflicting types
// class ConflictingIds {
//   id: string     // Can't satisfy both HasId (string) and HasNumId (int)
//   implements HasId {}
//   implements HasNumId {}
// }

// COMPILE ERROR: missing required method
// class IncompleteAnimal {
//   name: string
//   age: int
//   implements Animal {}   // speak() has no default and is not provided
// }

// COMPILE ERROR: circular requires
// interface Circular requires Circular {}

// COMPILE ERROR: ambiguous call without qualification
// test "ambiguous call" {
//   let h = Hybrid {}
//   h.encode()  // ERROR
// }
```

### Test Group 1: Basic Interface Implementation

```baml
testset "basic implementation" {
  test "required method implementation" {
    let d = Dog { breed: "Labrador", name: "Rex", age: 3 }
    assert.equal(d.speak(), "Woof!")
  }

  test "default method inherited" {
    let d = Dog { breed: "Labrador", name: "Rex", age: 3 }
    assert.equal(d.describe(), "Rex (age 3)")
  }

  test "default method overridden" {
    let c = Cat { indoor: true, name: "Whiskers", age: 5 }
    assert.equal(c.describe(), "Whiskers the indoor cat")
  }

  test "all fields accessible on class instance" {
    let d = Dog { breed: "Labrador", name: "Rex", age: 3 }
    assert.equal(d.name, "Rex")       // linked to Animal.name
    assert.equal(d.breed, "Labrador") // class-own
    assert.equal(d.age, 3)            // linked to Animal.age
  }

  test "empty implements block with all defaults" {
    let tl = TimestampLogger { prefix: "TS" }
    // Logger.log() has a default; TimestampLogger overrides it but calls default
    assert.contains(tl.log("hello"), "[LOG] hello")
    assert.contains(tl.log("hello"), "TS")
  }

  test "calling default from override" {
    let tl = TimestampLogger { prefix: "X" }
    assert.equal(tl.log("test"), "X [LOG] test")
  }
}
```

### Test Group 2: Interface Fields

```baml
testset "interface fields" {
  test "class fields satisfy interface contract" {
    let s = Server { max_connections: 50, host: "localhost", port: 8080 }
    assert.equal(s.host, "localhost")
    assert.equal(s.port, 8080)
    assert.equal(s.max_connections, 50)
  }

  test "field default makes construction optional" {
    let ds = DefaultServer {}
    assert.equal(ds.host, "localhost")
    assert.equal(ds.port, 8080)
  }

  test "field default can be overridden at construction" {
    let ds = DefaultServer { host: "prod.example.com", port: 443 }
    assert.equal(ds.host, "prod.example.com")
    assert.equal(ds.port, 443)
  }

  test "class fields satisfy interface contract through auto-linking" {
    // Duck declares name and age at class level; both are auto-linked
    // to Animal's field requirements.
    let d = Duck { color: "white", name: "Donald", age: 2 }
    assert.equal(d.name, "Donald")
  }

  test "interface-typed variable uses interface field names" {
    let d = Duck { color: "white", name: "Donald", age: 2 }
    let a: Animal = d
    assert.equal(a.name, "Donald")    // interface field name
    assert.equal(a.age, 2)
  }
}
```

### Test Group 3: Multiple Interface Implementation

```baml
testset "multiple interfaces" {
  test "class implements two interfaces" {
    let duck = Duck { color: "white", name: "Donald", age: 2 }
    assert.equal(duck.speak(), "Quack!")
    assert.equal(duck.swim(), "Donald swims gracefully")
    assert.equal(duck.speed(), 3.5)
  }

  test "unambiguous methods callable directly" {
    let duck = Duck { color: "white", name: "Donald", age: 2 }
    // speak() only in Animal, swim() only in Swimmer — no conflict
    duck.speak()
    duck.swim()
  }

  test "explicit qualification always works" {
    let duck = Duck { color: "white", name: "Donald", age: 2 }
    assert.equal(duck.as<Animal>.speak(), "Quack!")
    assert.equal(duck.as<Swimmer>.swim(), "Donald swims gracefully")
  }
}
```

### Test Group 4: Method Disambiguation

```baml
testset "disambiguation" {
  test "same method name, same signature — must qualify" {
    let h = Hybrid {}
    assert.equal(h.as<Serializer>.encode(), "json:{}")
    // h.encode() would be a compile error — tested in error cases
  }

  test "same method name, different return type — must qualify" {
    let h = Hybrid {}
    let s: string = h.as<Serializer>.encode()
    let i: int = h.as<IntSerializer>.encode()
    assert.equal(s, "json:{}")
    assert.equal(i, 42)
  }

  test "through interface-typed variable — unambiguous" {
    let h = Hybrid {}
    let ser: Serializer = h
    let isr: IntSerializer = h
    assert.equal(ser.encode(), "json:{}")
    assert.equal(isr.encode(), 42)
  }

  test "diamond — each block resolves independently" {
    let d = Diamond {}
    assert.equal(d.as<Left>.foo(), "Left")
    assert.equal(d.as<Right>.foo(), "Right")
    // d.foo() would be a compile error
  }
}
```

### Test Group 5: Interface as a Type

```baml
testset "interface as type" {
  test "interface-typed parameter" {
    let d = Dog { breed: "Labrador", name: "Rex", age: 3 }
    let result = describe_animal(d)
    assert.contains(result, "Rex")
  }

  test "interface-typed variable holds different concrete types" {
    let animals: Animal[] = [
      Dog { breed: "Lab", name: "Rex", age: 3 },
      Cat { indoor: true, name: "Whiskers", age: 5 },
      Duck { color: "white", name: "Donald", age: 2 },
    ]
    assert.equal(animals.length(), 3)
    assert.equal(animals[0].speak(), "Woof!")
    assert.equal(animals[1].speak(), "Meow.")
    assert.equal(animals[2].speak(), "Quack!")
  }

  test "interface-typed variable dispatches correctly" {
    let a: Animal = Cat { indoor: false, name: "Luna", age: 2 }
    assert.equal(a.speak(), "Meow.")
    assert.equal(a.name, "Luna")
  }

  test "interface field access through typed variable" {
    let a: Animal = Dog { breed: "Poodle", name: "Buddy", age: 4 }
    assert.equal(a.name, "Buddy")
    assert.equal(a.age, 4)
    // a.breed would be a compile error — breed is not on Animal
  }

  test "assigning to parent interface type" {
    let e = Employee { salary: 120000.0, name: "Alice", age: 30, occupation: "Engineer" }
    let named: Named = e
    assert.equal(named.name, "Alice")
    let aged: Aged = e
    assert.equal(aged.age, 30)
    let person: Person = e
    assert.equal(person.introduce(), "Alice, Engineer")
  }
}

function describe_animal(a: Animal) -> string {
  return a.name + " says " + a.speak()
}
```

### Test Group 6: Interface Requirements (`requires`)

```baml
testset "interface requirements" {
  test "class with separate implements blocks satisfies each interface" {
    let e = Employee { salary: 90000.0, name: "Bob", age: 25, occupation: "Designer" }

    let person: Person = e
    assert.equal(person.introduce(), "Bob, Designer")

    let named: Named = e
    assert.equal(named.name, "Bob")

    let aged: Aged = e
    assert.equal(aged.age, 25)
  }

  test "default from required interface inherited" {
    let e = Employee { salary: 200000.0, name: "Carol", age: 40, occupation: "CTO" }
    // introduce() is a default on Person
    assert.equal(e.introduce(), "Carol, CTO")
  }

  test "fields from required interfaces linked via class fields" {
    // Person requires Named (name) and Aged (age), adds occupation.
    // Employee declares all fields at class level and implements each.
    let e = Employee { salary: 110000.0, name: "Dan", age: 35, occupation: "PM" }
    assert.equal(e.name, "Dan")
    assert.equal(e.age, 35)
    assert.equal(e.occupation, "PM")
  }
}
```

### Test Group 7: Generics

```baml
testset "generic interfaces" {
  test "generic interface with concrete type param" {
    let stack = SimpleStack<int> { items: [] }
    stack.add(10)
    stack.add(20)
    assert.equal(stack.size(), 2)
    assert.equal(stack.get(0), 10)
  }

  test "generic bound enforced" {
    let animals: Animal[] = [
      Dog { breed: "Lab", name: "Rex", age: 3 },
      Cat { indoor: true, name: "Luna", age: 1 },
    ]
    let result = find_by_name<Animal>(animals, "Luna")
    assert.not_null(result)
    assert.equal(result.name, "Luna")
  }

  test "generic container with interface-typed elements" {
    let stack = SimpleStack<Animal> { items: [] }
    stack.add(Dog { breed: "Lab", name: "Rex", age: 3 })
    stack.add(Cat { indoor: true, name: "Luna", age: 1 })
    assert.equal(stack.size(), 2)
    assert.equal(stack.get(0).speak(), "Woof!")
  }
}

function find_by_name<T extends Named>(items: T[], target: string) -> T? {
  for (let item in items) {
    if (item.name == target) {
      return item
    }
  }
  return null
}
```

### Test Group 8: `match` on Interface Types

```baml
testset "match on interfaces" {
  test "match narrows interface to concrete class" {
    let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }
    let result = match (a) {
      a: Dog => "Dog: " + a.breed
      a: Cat => "Cat: " + (match (a.indoor) { true => "indoor", false => "outdoor" })
      _ => "Other: " + a.name
    }
    assert.equal(result, "Dog: Lab")
  }

  test "match catch-all required for open interface" {
    let a: Animal = Duck { color: "white", name: "Donald", age: 2 }
    let result = match (a) {
      a: Dog => "dog"
      a: Cat => "cat"
      _ => "other"
    }
    assert.equal(result, "other")
  }

  test "match destructures interface fields" {
    let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }
    let result = match (a) {
      Animal { name, age } => name + " is " + age.to_string()
    }
    assert.equal(result, "Rex is 3")
  }

  test "match on union of interface and primitive" {
    let vals: (Animal | string)[] = [
      Dog { breed: "Lab", name: "Rex", age: 3 },
      "plain string",
    ]
    for (let v in vals) {
      let _ = match (v) {
        Animal { name } => "animal: " + name
        s: string => "string: " + s
      }
    }
  }
}
```

### Test Group 9: LLM Functions

```baml
testset "llm functions" {
  test "llm function returning interface type" {
    // This test validates that the LLM can return any implementor of Animal
    let result = DetectAnimal(image.from_url("https://example.com/dog.jpg"))
    // result is Animal — could be Dog, Cat, or Duck
    assert.not_null(result.name)
    assert.is_true(result.age >= 0)
  }
}

function DetectAnimal(img: image) -> Animal {
  client GPT4o
  prompt #"
    Identify the animal in the image.
    {{ ctx.output_format }}
  "#
}
```

### Test Group 10: Self Access and Cross-Block Calls

```baml
testset "self access" {
  test "self accesses class fields" {
    let d = Dog { breed: "Labrador", name: "Rex", age: 3 }
    // speak() uses self.name internally
    assert.equal(d.speak(), "Woof!")
  }

  test "default method accesses class fields linked to interface" {
    let d = Dog { breed: "Labrador", name: "Rex", age: 3 }
    // describe() default uses self.name and self.age (class fields linked to Animal)
    assert.equal(d.describe(), "Rex (age 3)")
  }

  test "class own method accessible from implements block" {
    let s = Server { host: "localhost", port: 8080 }
    assert.equal(s.address(), "localhost:8080")
  }

  test "cross-block call flattens when unambiguous" {
    let p = Polite { name: "Alice" }
    assert.equal(p.greet(), "Hello, I'm Alice")        // unambiguous — direct call
    assert.equal(p.as<Greeter>.greet(), "Hello, I'm Alice") // explicit also works
    assert.contains(p.bye(), "Hello, I'm Alice")        // bye() calls self.greet() internally
    assert.contains(p.bye(), "goodbye")
  }
}
```

### Test Group 11: Edge Cases

```baml
testset "edge cases" {
  test "interface with only fields, empty implements block" {
    let ds = DefaultServer {}
    assert.equal(ds.host, "localhost")
  }

  test "class with many interfaces" {
    let d = Duck { color: "white", name: "Donald", age: 2 }
    let animal: Animal = d
    let swimmer: Swimmer = d
    assert.equal(animal.speak(), "Quack!")
    assert.equal(swimmer.swim(), "Donald swims gracefully")
  }

  test "interface-typed array with heterogeneous concrete types" {
    let animals: Animal[] = [
      Dog  { breed: "X",      name: "A", age: 1 },
      Cat  { indoor: true,    name: "B", age: 2 },
      Duck { color: "yellow", name: "C", age: 3 },
    ]
    let sounds = animals.map((a) => a.speak())
    assert.equal(sounds, ["Woof!", "Meow.", "Quack!"])
  }

  test "default method calls another default method" {
    // Printable.verbose() calls self.display() which is also a default
    let item = Item { name: "Widget" }
    assert.equal(item.display(), "[Widget]")
    assert.equal(item.verbose(), "Printable([Widget])")
  }

  test "override one default, inherit another" {
    let user = User { name: "Alice", email: "alice@example.com" }
    assert.equal(user.display(), "[Alice]")  // inherited default
    assert.equal(user.verbose(), "User(Alice, alice@example.com)")  // overridden
  }

  test "generic bound with intersection" {
    // Demonstrates <T extends A & B> if we add intersection support to extends bounds
    // Deferred — see Open Questions
  }

  test "deeply nested interface inheritance" {
    // A requires B, B requires C, C requires D
    // Class implements A AND B AND C AND D — each separately
    // Validates the explicit requires-fulfillment rule
  }
}
```

### Test Group 12: Field Linking with `as`

```baml
testset "as field linking" {
  test "explicit as linking maps interface fields to class fields" {
    let d = DualNamed { real_name: "Alice", label_name: "ALICE-001" }
    assert.equal(d.real_name, "Alice")
    assert.equal(d.label_name, "ALICE-001")
  }

  test "interface-typed variable resolves through as link" {
    let d = DualNamed { real_name: "Alice", label_name: "ALICE-001" }
    let n: Named = d
    assert.equal(n.name, "Alice")       // Named.name → real_name
    let l: Labeled = d
    assert.equal(l.name, "ALICE-001")   // Labeled.name → label_name
  }

  test "auto-linking when names match" {
    // Employee has `name: string` at class level; `implements Named {}` auto-links it
    let e = Employee { salary: 50000.0, name: "Bob", age: 30, occupation: "Dev" }
    let n: Named = e
    assert.equal(n.name, "Bob")
  }
}
```

### Test Group 13: Upcasting with `.as<I>`

```baml
testset "upcasting" {
  test "explicit upcast with .as<I> on concrete type" {
    let d = Dog { breed: "Lab", name: "Rex", age: 3 }
    assert.equal(d.as<Animal>.speak(), "Woof!")
  }

  test "implicit upcast via variable assignment" {
    let d = Dog { breed: "Lab", name: "Rex", age: 3 }
    let a: Animal = d
    assert.equal(a.speak(), "Woof!")
    assert.equal(a.name, "Rex")
  }

  test "multi-instantiation disambiguation with .as<I>" {
    let m = MultiFormat { data: "42" }
    // m.convert() would be ambiguous — Converter<int> vs Converter<float>
    assert.equal(m.as<Converter<int>>.convert(), 42)
    assert.equal(m.as<Converter<float>>.convert(), 42.5)
  }

  test "multi-instantiation via interface-typed variable" {
    let m = MultiFormat { data: "42" }
    let ci: Converter<int> = m
    let cf: Converter<float> = m
    assert.equal(ci.convert(), 42)
    assert.equal(cf.convert(), 42.5)
  }

  test "downcast requires match, not as" {
    let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }
    // a.as<Dog> would be a compile error — as is for upcasts only
    let result = match (a) {
      d: Dog => d.breed
      _ => "unknown"
    }
    assert.equal(result, "Lab")
  }
}
```

### Test Group 14: Reflection (`reflect.type_of`)

```baml
testset "reflection basics" {
  test "type_of interface returns interface type" {
    let t = reflect.type_of<Animal>()
    assert.equal(t.to_string(), "Animal")
  }

  test "type_of class returns class type" {
    let t = reflect.type_of<Dog>()
    assert.equal(t.to_string(), "Dog")
  }

  test "interface type != class type" {
    assert.is_true(reflect.type_of<Animal>() != reflect.type_of<Dog>())
  }

  test "same interface type is equal" {
    assert.equal(reflect.type_of<Animal>(), reflect.type_of<Animal>())
  }

  test "type_of generic interface includes type arg" {
    let t = reflect.type_of<Container<int>>()
    assert.equal(t.to_string(), "Container<int>")
  }

  test "generic interface different type args are not equal" {
    assert.is_true(reflect.type_of<Container<int>>() != reflect.type_of<Container<string>>())
  }
}

testset "reflection implements" {
  test "class type implements interface type" {
    let dog = reflect.type_of<Dog>()
    let animal = reflect.type_of<Animal>()
    assert.is_true(dog.implements(animal))
  }

  test "class type does not implement unrelated interface" {
    let dog = reflect.type_of<Dog>()
    let swimmer = reflect.type_of<Swimmer>()
    assert.is_true(!dog.implements(swimmer))
  }

  test "class implementing multiple interfaces" {
    let duck = reflect.type_of<Duck>()
    assert.is_true(duck.implements(reflect.type_of<Animal>()))
    assert.is_true(duck.implements(reflect.type_of<Swimmer>()))
  }

  test "implemented_by is reverse of implements" {
    let dog = reflect.type_of<Dog>()
    let animal = reflect.type_of<Animal>()
    assert.equal(dog.implements(animal), animal.implemented_by(dog))
  }

  test "reflection sees every directly-implemented interface" {
    let emp = reflect.type_of<Employee>()
    assert.is_true(emp.implements(reflect.type_of<Person>()))
    assert.is_true(emp.implements(reflect.type_of<Named>()))
    assert.is_true(emp.implements(reflect.type_of<Aged>()))
  }

  test "interface does not implement itself" {
    let animal = reflect.type_of<Animal>()
    assert.is_true(!animal.implements(animal))
  }

  test "primitive does not implement user interface" {
    let int_t = reflect.type_of<int>()
    let animal = reflect.type_of<Animal>()
    assert.is_true(!int_t.implements(animal))
  }
}

testset "reflection implementors" {
  test "interface lists its implementors" {
    let animal = reflect.type_of<Animal>()
    let impls = animal.implementors()
    assert.is_true(impls.length() >= 3)  // Dog, Cat, Duck at minimum
  }

  test "implementors contains expected classes" {
    let animal = reflect.type_of<Animal>()
    let impls = animal.implementors()
    assert.is_true(impls.contains(reflect.type_of<Dog>()))
    assert.is_true(impls.contains(reflect.type_of<Cat>()))
    assert.is_true(impls.contains(reflect.type_of<Duck>()))
  }

  test "non-interface type returns empty implementors" {
    let int_t = reflect.type_of<int>()
    assert.equal(int_t.implementors(), [])
  }

  test "class type returns empty implementors" {
    let dog = reflect.type_of<Dog>()
    assert.equal(dog.implementors(), [])
  }
}

testset "reflection in generic context" {
  test "type_of<T> inside generic function with interface bound" {
    let result = type_name_of<Dog>()
    assert.equal(result, "Dog")
  }

  test "implements check inside generic function" {
    assert.is_true(does_implement_animal<Dog>())
    assert.is_true(does_implement_animal<Duck>())
    assert.is_true(!does_implement_animal<int>())
  }
}

function type_name_of<T extends Animal>() -> string {
  reflect.type_of<T>().to_string()
}

function does_implement_animal<T>() -> bool {
  reflect.type_of<T>().implements(reflect.type_of<Animal>())
}
```

### Test Group 15: Compile Error Cases

These test that the compiler rejects invalid code. Each case should produce a diagnostic.

```baml
testset "compile errors" {
  // These tests validate compiler diagnostics.
  // In a real test runner, these would be "expect compile error" tests.

  test "missing required method is compile error" {
    // class Incomplete {
    //   name: string
    //   age: int
    //   implements Animal {}
    //   // ERROR: missing required method `speak() -> string`
    // }
    assert.is_true(true)  // placeholder — actual test is the compiler rejecting the class
  }

  test "field type mismatch is compile error" {
    // class BadField {
    //   port: string         // Config requires int — type mismatch
    //   implements Config {}
    // }
    assert.is_true(true)
  }

  test "conflicting field types with no explicit linking is compile error" {
    // class BadIds {
    //   id: string           // satisfies HasId, but HasNumId wants int
    //   implements HasId {}
    //   implements HasNumId {}
    //   // ERROR: field `id` has type `string` but HasNumId requires `int`.
    //   // Fix: declare `str_id: string` and `num_id: int`, then use
    //   // `implements HasId { id as str_id }` and
    //   // `implements HasNumId { id as num_id }`
    // }
    assert.is_true(true)
  }

  test "ambiguous method call without qualification is compile error" {
    // let h = Hybrid {}
    // h.encode()
    // ERROR: ambiguous — encode() defined in both Serializer and IntSerializer
    assert.is_true(true)
  }

  test "circular interface requires is compile error" {
    // interface Loop requires Loop {}
    // ERROR: circular interface inheritance
    assert.is_true(true)
  }

  test "wrong return type in implements block is compile error" {
    // class BadReturn {
    //   name: string
    //   age: int
    //   implements Animal {
    //     function speak(self) -> int { 42 }
    //     // ERROR: expected `-> string`, found `-> int`
    //   }
    // }
    assert.is_true(true)
  }

  test "wrong parameter type in implements block is compile error" {
    // interface Adder {
    //   function add(self, a: int, b: int) -> int
    // }
    // class BadAdder {
    //   implements Adder {
    //     function add(self, a: string, b: string) -> int { 0 }
    //     // ERROR: parameter types don't match
    //   }
    // }
    assert.is_true(true)
  }

  test "implements non-existent interface is compile error" {
    // class Ghost {
    //   implements DoesNotExist {}
    //   // ERROR: interface `DoesNotExist` not found
    // }
    assert.is_true(true)
  }

  test "accessing concrete-class field through interface type is compile error" {
    // let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }
    // a.breed
    // ERROR: `breed` is not a member of interface `Animal`
    assert.is_true(true)
  }

  test "duplicate implements block for same interface is compile error" {
    // class Dup {
    //   name: string
    //   age: int
    //   implements Animal { function speak(self) -> string { "a" } }
    //   implements Animal { function speak(self) -> string { "b" } }
    //   // ERROR: duplicate `implements Animal` block
    // }
    assert.is_true(true)
  }

  test "missing field with no as link is compile error" {
    // interface Config { host: string, port: int }
    // class Broken {
    //   host: string
    //   // no port field declared
    //   implements Config {}
    //   // ERROR (E0113): class `Broken` is missing field `port: int`
    //   // required by interface `Config`
    // }
    assert.is_true(true)
  }

  test "subtype field declaration is compile error (invariance)" {
    // interface AnimalNode { parent: AnimalNode? }
    // class Dog {
    //   parent: Dog?          // Dog? is subtype of AnimalNode?, but invariance rejects it
    //   implements AnimalNode {}
    //   // ERROR (E0116): expected `parent: AnimalNode?`, found `parent: Dog?`
    // }
    assert.is_true(true)
  }

  test "as downcast is compile error" {
    // let a: Animal = Dog { breed: "Lab", name: "Rex", age: 3 }
    // a.as<Dog>    // ERROR: .as<T> is for upcasts only. Use match to downcast.
    assert.is_true(true)
  }

  test "ambiguous multi-instantiation call without as is compile error" {
    // let m = MultiFormat { data: "42" }
    // m.convert()  // ERROR (E0121): ambiguous — Converter<int> vs Converter<float>
    assert.is_true(true)
  }

  test "multi-Self method on interface-typed receiver is compile error" {
    // interface Cloneable { function clone(self) -> Self }
    // let a: Cloneable = ...
    // a.clone()  // ERROR: `clone` has multiple `Self` uses; requires concrete receiver
    assert.is_true(true)
  }

  test "missing required interface is compile error" {
    // interface Person requires Named, Aged { occupation: string }
    // class Bad {
    //   name: string
    //   occupation: string
    //   implements Person {}
    //   // ERROR (E0125): class `Bad` implements `Person`, which requires
    //   // `Named` and `Aged`, but `Bad` does not implement them.
    // }
    assert.is_true(true)
  }
}
```

## Open Questions

1. **Intersection bounds.** Should `<T extends Named & Aged>` be supported, or should users write two separate generic parameters with overlapping bounds? `&` is TS-native but introduces ambiguity with bitwise-and in expression position; needs disambiguating grammar.

2. **`default` keyword scope.** Is `default.method()` sufficient, or do we need `default<InterfaceName>.method()` when an interface requires another interface that also has a default for the same method name?

3. **Method visibility.** Should interface methods be callable on the class without qualification even when there's no ambiguity, or should all interface methods always require `obj.as<Interface>.method()`? (This BEP proposes: unambiguous = direct call OK.)

4. **Sealed interfaces.** Deferred to a follow-up BEP. When added, they would enable exhaustive `match` without `_` and freeze the implementor set.

5. **Ordering of `implements` blocks.** Does declaration order matter? (Proposed: no — order is irrelevant for semantics.)

6. **Interaction with streaming (BEP-006/009).** When an LLM function returns an interface type and we're streaming, the parser needs to identify the concrete type early. Does this require a discriminator field?

## Summary of Design Decisions

| Axis | Decision |
|---|---|
| Syntax keyword | `interface` |
| Conformance | Nominal — `implements I` (or `implement I`) required |
| Keyword forms | `implements` and `implement` both accepted; `implements ... for T` works outside any class body |
| Implementation scoping | `implements I { ... }` inside class body, or `implements I for T { ... }` at top level |
| Default methods | Yes — body in interface is a default |
| Calling defaults from override | `default.method()` |
| `self` access | Flattened — `self.field` / `self.method()` for anything unambiguous; qualify only when ambiguous |
| Field declaration | Fields live on the class (class-flat). `implements` block auto-links by name; `name as class_field` for explicit linking |
| Field construction | All fields use bare class-level names: `MyClass { field: value }` |
| Field access | `obj.field` for class fields; through interface-typed variable, interface field names resolve via link |
| Field conflict (mismatched type) | Compile error (E0116) — invariant type match required |
| Field conflict (two interfaces, same name) | Author chooses: share one class field (auto-link both) or use separate fields with `as` linking |
| Method disambiguation | `obj.as<InterfaceName>.method()` when ambiguous |
| Unambiguous method call | Direct `obj.method()` allowed |
| Interface-typed variable | Always unambiguous (type is the disambiguator) |
| Interface requirements | `requires`, multiple, implementor fulfills each separately |
| Diamond problem | Avoided — each `implements` block resolves independently |
| Generic interfaces | Yes — `interface Container<T>` |
| Generic bounds | `<T extends TypeExpr>` — TS-style; accepts any type expression (interfaces, primitives, unions, optionals, arrays, generic instantiations) |
| Upcasting | Implicit via `let a: I = x`; explicit via `x.as<I>.method()` (method-style). No `as` for downcasts — use `match` / `is` |
| Variance | Invariant in v1 |
| Reflection | `type_of<I>()`, `.implements()`, `.implemented_by()`, `.implementors()` |
| Sealed interfaces | Deferred |
| LLM return type | Allowed — implementors rendered as `oneOf` |
| Codegen — Python | Pydantic base class |
| Codegen — TS | Native `interface` + `implements` |
| Codegen — Rust | `trait` + `impl Trait for Type` |
| External trait impls | Not in this BEP |
| Subtyping | Concrete types (class, primitive, enum) can implement interfaces; non-concrete types (union, interface, `unknown`) cannot. Implicit upcasting for subtype→supertype only. `int <: bigint`; `int` to `float` requires explicit cast. Generics invariant. |
| `Self` types | Fully static, no runtime checks. Methods with multiple `Self` params require concrete-typed receiver. Return type is concrete when receiver is concrete, collapses to interface type otherwise. `self` counts as `Self`. Extends to static methods (future). |
