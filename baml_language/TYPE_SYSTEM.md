This doc is primarily written for use in development work on the BAML programming language, but may also be useful to users of the language.

At the time of this document's writing, not all BAML type system features work as specified here. This document is thus prescriptive, not descriptive: this is how the type system _should_ work. Any features or implementations that rely on preexisting incomplete behavior should be considered liable to change.

# The BAML Type System

The BAML programming language has a strong, statically checked, dynamically monomorphized type system. While it uses TypeScript-like syntax, it provides stronger guarantees and identity which also permit safe runtime validation (e.g. `match` over union types).

BAML has a set-theoretic type system. It has unions like TypeScript (but more strongly typed) and type bound-based interfaces like Rust traits/Swift protocols. This enables flexibility while upholding correctness.

Super/sub-typing in BAML represents a super/sub-set relationship, not inheritance. While concrete types may receive the default method implementations from interfaces, the flat Rust-like [interface system](https://beps.boundaryml.com/beps/44) avoids the diamond inheritance problem.

## Implementation & Design Guidelines

1. The golden rule: runtime values may never violate their compile-time type contracts. The type system is not a suggestion-- it is extant and must be accurate at all times.
2. Types exist at run-time. [Reflection](https://beps.boundaryml.com/beps/39) is a first-class feature of BAML. We can always determine what concrete type a value has, as well as if the value fits a given non-concrete type.
3. Prefer compiler errors over type-erasure: the compiler is here to ensure users catch problems early instead of having run-time errors (or worse, silently producing the incorrect answer).

## Taxonomy

BAML types can be categorized into three groups:

- Concrete types: these are the types that "exist" at run-time: `int`s, `float`s, other primitives, various classes, enums, etc.
- Abstract types: unions, interface-types (pure existential types paralleling [Rust `dyn Trait`](https://varkor.github.io/blog/2018/07/03/existential-types-in-rust.html)), and `unknown`.
- Literal types: these are subtypes of the concrete types, generally a single member of the set. For example, `1`, `"some string"`, `true`, or `MyEnum.MyVariant` when used as a type.

There is also the bottom-type `never`, which represents a type with no members.

### Concrete Types

Every run-time value is of exactly one concrete type. They are the types that define a memory layout and specific method implementations. They include all primitives, all class types, all enum types, all function types, and a few special built-in types (such as `Future` or the reflective `type` type). These generally correspond to an `Object` (or inline primitive) in the BEX VM.

As of writing, no concrete types have subtyping relationships with each other. This is because doing so would require a physical conversion-- they cannot be statically cast as their memory layouts differ. So where we could statically upcast an `int` to `baml.ops.Equals` (which accepts any implementing concrete type), we cannot statically convert an `int` to a `float`. Additionally, since the concrete types represent different things, static conversion would generally not be well-defined (e.g. `int`/`float` conversion will always be lossy and/or fallible in either direction). For this reason, it's best to require explicit conversions. To avoid this being too inconvenient, we define operators like `int + float` that do the conversions while still clearly being a computation point.

The stdlib declares _companion carrier classes_ for the builtins: `baml.Int`, `baml.Bigint`, `baml.Float`, `baml.Bool`, `baml.String`, `baml.Null`, `baml.Uint8Array`, `baml.Array<T>`, and `baml.Map<K, V>`. A carrier is a stand-in for declaring inherent members on a builtin type (morally `implement int { ... }`, which the syntax does not support yet) and **effectively does not exist as a type**: every occurrence of the spelling denotes the builtin type itself — `baml.Int` _is_ `int`, `baml.Array<T>` _is_ `T[]` — in every position, including `Self` inside the carrier's own methods and `implements` blocks and an impl's `for` target. No value ever inhabits a nominal carrier class.

Two companions are the opposite case — the class name IS the builtin type's canonical spelling: `reflect.Type` (the metatype) and `baml.future.Future<V, E>`. Each denotes its dedicated type kind; the class declaration exists to carry members and documentation. Their long-term shape is undecided (perhaps a magic-builtin-backed alias with an inherent `implement` block), but the invariant holds either way: the spelling denotes the builtin kind, and the declaration only attaches members.

### Abstract Types

Abstract types can generally be viewed as set unions of different groups of concrete (and literal) types:

- Unions: the name is self-explanatory. It's a union of types.
- Interface-types: represent the union of all concrete types that implement the interface's contract. Reading about [existential types](https://homepages.inf.ed.ac.uk/gdp/publications/Abstract_existential.pdf) may be helpful and/or interesting for understanding the theoretical basis for BAML's interface-types.
- `unknown`: this is the union of all types. All values are members of this type.

### Literal Types

Literal types (here named due to currently all corresponding to literals) represent subsets of concrete types. They correspond to exactly one concrete type (literal type `1` is of concrete type `int`) and use its methods. Whereas abstract types represent a union relationship between types, literal types are a type that constrains the possible values out of a concrete type (currently only to a single member, without defining a union thereof)

### `never`

`never` is BAML's bottom type. It represents the empty set: there are no values of this type. As the empty set is a subset of all other sets, `never` is a subtype of all types in BAML.

As an empty set, `never` can always be omitted from union types: `int | never` is equivalent to `int`. A function that returns `never` cannot return, and a function with `throws never` can never throw an error (though it can still panic).

## Subtyping Rules

Subtyping in BAML is subset-based, not inheritance-based.

### Variance

When a type takes in type parameters (including but not limited to generics), subtyping becomes complicated. [Variance](https://en.wikipedia.org/wiki/Type_variance) describes how type parameters affect subtyping of the parameterized type.

BAML's type system has the following properties:

1. type parameters on classes/arrays/maps (generics) are invariant.
   If they were not invariant, the following case would cause the type contract to be invalid:
   ```baml
   // THIS CODE IS FOR DEMONSTRATIVE PURPOSES; IT IS INVALID BAML
   let a: int[] = [1, 2, 3];
   let b: (int | string)[] = a; // if covariant, this would be valid
   b[1] = "hello"; // The type `int | string` allows `string`
   let c: int = a[1]; // oops!!!!! it's actually a `string`
   ```
   This is a major point in which TypeScript fails: it would permit the above unsound code, resulting in `c` having a potentially cascading type/state mismatch.
2. Union members are covariant: all members are subtypes of the union (as are their subtypes).
   1. Same with interfaces and `unknown`
   2. Literal types behave as union members within concrete types
3. Function types:
   1. Arguments must be contravariant (function A is subtype only if its arguments are supertype)
      - This includes optional arguments: function A is subtype only if its set of optional arguments are a superset and the type of each of them is a supertype.
   2. Return and error types must be covariant (function A is subtype only if its return and error types are subtype)
   3. Example:

      ```baml
      function foo(a: int | string) -> bool throws never { true }

      //////// EXAMPLE CALLSITE ////////
      /// args of `foo` are more permissive to caller so it is valid:
      /// return type of `foo` is less expectant of caller so it is valid:
      let a: (int) -> bool | float throws never = foo;
      ```

### BAML Subtyping Cases

Literals are subtypes which together comprise their concrete types:

- `(int.min_value() | ... | -2 | -1 | 0 | 1 | 2 | ... | int.max_value()) == int`
  - Note that the extrema are not valid BAML type syntax, one would have to write out the full literal.
- `(true | false) == bool`
- `("" | "\x00" | "\x01" | ... | "all strings" | ...) == string` (infinite size set)
- `(... | -2n | -1n | 0n | 1n | 2n | ...) == bigint` (infinite size set)
- `(Enum.VariantA | Enum.VariantB | ...) == Enum` (for each enum type)

Other subtypes:

- bottom type: `never <: T` for all `T`
- top type: `T <: unknown` for all `T`
- unions: `T <: (T | ...)` for all `T`
  - Including union subsets: `(T0 | T1) <: (T0 | T1 | ...)`
  - Also including optionals: `T? := T | null`
- identity: `T =: T` for all `T`
- interface bounds: `A <: B` for interfaces `A` and `B` where `A requires B` (for interface-existential types)
- interface membership: `C <: I` for concrete type `C` that implements interface `I` (for interface-existential type)
- functions: `((a0, a1, ..., ao0=..., ...) -> ar throws ae) <: ((b0, b1, ..., bo0=..., ...) -> br throws be)` if and only if:
  - Same number of positional arguments where each `bN <: aN` (contravariant)
  - Each optional argument ([BEP-033](https://beps.boundaryml.com/beps/33)) `boN` has a corresponding `aoN` of the same name where `boN <: aoN` (contravariant, subset function type has superset of optional args)
  - Return type: `ar <: br` (covariant)
  - Error type: `ae <: be` (covariant)

In nearly all contexts, we can thus simplify types and maintain equivalence: `1 | int == int | 1 == int`, `anything | unknown == unknown`, `true | false == bool`, `Side.Left | Side.Right == Side` (enum), etc. The only exceptions are places like LLM function return types where the order affects the parsing. However, this should only matter in this one location based on explicit declaration for the SAP parser, and can still be treated the same for in-language type algebra.

## Type Aliases and Recursive Types

Type aliases are transparent (structural, not nominal): `type X = T` makes `X` the very same type as `T` in every context. The alias name is a spelling device, never an identity. Non-recursive aliases are expanded during lowering; only recursive aliases survive as named references, and the name still carries no identity of its own.

### Equirecursive identity

Recursive aliases are equirecursive μ-types over regular trees: a recursive alias denotes the unique regular tree obtained by unfolding its definition, and two types are the same type if and only if their trees are equal under the type algebra. Alias names, unfolding depth, and spelling never contribute identity:

```baml
type A = int | A[]
type B = int | B[]
// A and B are the same type: mutual subtypes, interchangeable everywhere.
// So are `A`, `int | A[]`, and `int | (int | A[])[]` (partial unfoldings),
// and mutually recursive definitions that unfold to the same tree.
```

### Values and membership

BAML values are finite object graphs and may be self-referential (`let a: A = []; a.push(a);` is valid for `type A = A[]`). Membership checking never recurses over a value: every value carries (can trivially reconstruct) its concrete run-time type — for example, a list holds its element type as metadata, and writes into it are statically checked against that element type — so a value `v` is a member of type `T` if and only if `v`'s concrete type is a subtype of `T`. All recursion in the system lives in the type-level subtyping relation, which is decided coinductively over regular trees.

### Productivity

Because values can be self-referential, membership is not a well-founded judgment and subtyping over recursive types is coinductive. Coinductive derivations must be productive: every cycle in a derivation must pass through a type constructor. A self-supporting cycle ("`t <: A` because `t <: A`", reachable through an unguarded recursive union member) proves nothing. Consequently, an unguarded recursive union member contributes nothing to its type, and a fully unguarded cycle is uninhabited:

```baml
type A = A | A[]   // the same type as `type L = L[]`: the unguarded `A` member is vacuous
type B = B         // denotes `never` (and is a compile error, E0068)
```

(The E0068 cycle check is per-SCC — a cycle group needs at least one list/map edge — so types like `A` above are legal; the algebra gives them the productive semantics.)

### Equivalence and canonical forms

Two types are **equivalent** if and only if they are mutual subtypes — they denote the same set of values. The canonical type algebra decides equivalence by structural equality of canonical forms, and for the algebra's rules these provably coincide with mutual subtyping (canonical forms are unique representatives of the equirecursive equivalence class).

Two deliberate carve-outs qualify the "if and only if" (both are compiler devices, not value-set semantics): the error-recovery sentinel types are bidirectionally *compatible* with everything to suppress diagnostic cascades but equivalent only to themselves, and a fact set containing a mutual `requires` cycle between distinct interfaces would make them mutual subtypes while remaining nominally distinct — such cycles are rejected at declaration (E0118), so the case arises only for artificially constructed fact sets.

`normalize` renders the canonical form back as surface syntax, with the following contract: the output is idempotent (`normalize(normalize(t)) == normalize(t)`), always equivalent to its input, and head-exposed (the root of a recursive alias is unfolded once; nested recursion stays folded as an alias name). Because surface syntax spells recursion via alias names, the rendering is canonical only up to the naming of recursion back-references: `normalize(A)` and `normalize(B)` above are equivalent but each spells back-references with its own name. Canonical *identity* is the equivalence judgment, not syntactic equality of rendered output.

## Pattern Matching

Pattern matching ([BEP-015](https://beps.boundaryml.com/beps/15)) on types falls naturally out of BAML's subset-based subtyping system. For any given value, BAML can determine whether a value is a member of a given type. A `match` expression with type-pattern arms can be thought of as checking each arm in order to see if the value is a member of the type-set (though various optimizations allow us to implement this more efficiently in many cases).

To check pattern fallibility/infallibility/exhaustiveness, BAML ensures that the union of the matched patterns represent a superset of the scrutinee's type, or defines a diverging path for fallible patterns if not.
`match` also rejects arms that can provably never be reached. While these do not necessarily violate the type system, they are dead code that could be misleading. A `match` arm `A(N)` is unreachable if and only if `SCRUTINEE ∩ (A(0) ∪ A(1) ∪ ... ∪ A(N-1)) = SCRUTINEE ∩ (A(0) ∪ A(1) ∪ ... ∪ A(N-1) ∪ A(N))`. If we cannot prove either way, we err towards saying it is reachable (for example, we exclude all arms with an `if` expression-guard from consideration in reachability and say they do not contribute to exhaustiveness as we can make no guarantees about the evaluated expression).

## Type Variables

There are several types of type variables in BAML: generics, associated types, and `Self`. At any run-time usage site, they correspond to exactly one _realized_ type. They (may) also have interface-bounds which enable us to use polymorphic behavior on them as a [universal type](https://en.wikipedia.org/wiki/Parametric_polymorphism).

- Generics are used in [type constructors](https://en.wikipedia.org/wiki/Type_constructor) ("generic types") and as type parameters on functions. The type they hold is provided by the caller.
- Associated types are used in interfaces. The type they hold is provided by the interface implementor.
- `Self` is used in interfaces to refer to the concrete implementor type
  - If the concrete type is generic, it refers to the monomorphized type (the result of the type constructor, rather than the type constructor itself: `Foo<int>`/`Foo<string>`/..., not `Foo<T>`)
  - In an interface's default method implementation, it behaves a bit like an associated type with the interface as its bound, since it may be realized to any implementing type.

As previously noted, type variables with interface bounds can only be filled by concrete types. This includes `Self`: when used in a class body, it must correspond to a specialization of the concrete class type, and when in an `interface`/`implements` block it is a type variable bounded by the interface.

### Generics on Functions

NOTE: for demonstrative purposes, I have added the unsupported syntax `dyn` to disambiguate when referring to an interface-existential type vs the interface itself.

```baml
function foo<T>(a: T) -> T {
	a
}

function bar<T extends baml.ops.Add>(a: T, b: T) -> T {
	a + b
}
```

BAML has generic type parameters. When a generic function is called, the caller provides realized[^1] type arguments for each parameter. While they can often be inferred, they must still be known unambiguously at the call site. In terms of identity, `bar<int>` can be considered a distinct, non-equivalent function from `bar<string>`, even if the implementation does not statically monomorphize. We call `bar<int>` a _specialization_ of `bar`.

It may be helpful to think of it this way: function `bar` takes in three parameters. One is a type-parameter `T` and the others are value-parameters `a` and `b`. We check statically during compilation to ensure that whatever `T` ends up being, it will be valid at run-time. Like how value-parameters are constrained by a type (`a: T`), a type-parameter may be constrained by interfaces (`T extends baml.ops.Add`). At some call sites, we may know at compile time what we are passing as `T`. At others it will depend on the caller's own type arguments. When interacting with the outside world (e.g. bridges), we may have to validate and pass it dynamically.

When a type-parameter is constrained by interfaces, it gains the additional constraint that it must be a concrete type. The reasoning: say we have an interface `interface AdditiveIdentity requires Add { ... }`. Then the interface-existential type `dyn AdditiveIdentity` is a subtype of interface-existential `dyn baml.ops.Add` (all instances of types that implement `AdditiveIdentity` must implement `baml.ops.Add`). However, if we called `bar<dyn AdditiveIdentity>(...)` with `T=dyn AdditiveIdentity` then the operation `a + b` would be invalid: we cannot guarantee that `a` and `b` are the same concrete type. For example

```baml
let a: dyn AdditiveIdentity = 3; // concrete type is `int`
let b: dyn AdditiveIdentity = baml.time.Instant.from_seconds(30n); // concrete type is `baml.time.Instant`
bar<dyn AdditiveIdentity>(a, b); // BAD: what is `int + baml.time.Instant`? There is no implementation of `baml.ops.Add` that does that.
```

As a result, while an unconstrained type-parameter like in `foo<T>` may be any type (including unions, interface-existentials, literals, etc.), a constrained type parameter must be concrete. This rule is also conceptually similar to how interfaces work generally: while a union may be a subtype of an interface-existential type if all members are, they are not an _implementor_ of the interface as only concrete types can implement interfaces.

[^1]: Realized types: that is, they are fully-known types at call-time. While you can pass a type arg `T` from a generic caller into a generic callee, that parameter `T` will hold some type at runtime, the same as any normal function parameter holds a value at runtime.

### Realized vs Unrealized Types

As used in BAML, "realized" types are types which contain _no_ type variables (fully monomorphized types). Unrealized types contain type variables (are still polymorphic). Types are always realized when they are used at run-time, either statically or via runtime binding of type parameters (generics).

In the following example, the unspecialized generic function `foo` uses unrealized type `Box<T>`.

```baml
function foo<T>(a: T) -> Box<T> {
	let out: Box<T> = Box { item: a };
	out
}
```

However, whenever a call occurs, `foo` will have been specialized: `foo<int>` passes a type parameter binding `T=int`, causing the types to be fully realized: `(int) -> Box<int>`.
Internally, when we call the specialized function `foo<int>(123)`, the bound type parameter `T=int` applies everywhere it is used as a type variable (meaning the type of `out` is fully realized within the context of the function call). In effect, we act _as if_ the types were realized via function monomorphization when the type parameter was passed, whether or not the implementation actually does so.

This is best illustrated by BAML's runtime reflection:

```baml
function asdf<T>() -> reflect.Type {
	reflect.Type.of<T>() // `Type.of` takes the type parameter and returns it as a value
}

////////

asdf<int>() // should return type `int`
// we passed type parameter `int` into `asdf`, which passed it into `reflect.Type.of`, which returned it as a value.
```

**How does this actually get run under the hood?**
For the function call `foo<int>(123)`, the run time `foo` function effectively receives two parameters: type argument `T=int` and value `a=123`. The compiler has ensured that the bounds for each are correct.

- The constructor for `Box` is called, with type parameter `T -> int` (inferred and filled in by the compiler) and value `a -> 123`.
- The value is assigned to `out`.
- We return `out`.
  The compiler has ensured that we behaved soundly and the caller correctly bounded the value-parameter by the type-parameter.

### Generics on Types (Type Constructors)

A type constructor, as the name suggests, constructs types! They are very familiar to us as generics on types: `Box<int>` has type constructor `Box` which takes in type parameter `int` and produces a realized type (bindings all type parameters is also called _monomorphization_). BAML doesn't permit `Box` by itself as a type: while BAML syntax permits it in some locations, this is only valid if we can unambiguously infer the parameters. Since BAML generics are invariant, `Box<A>` has a subtyping relationship with `Box<B>` if and only if `A == B`.

### Associated Types

Associated types are uniquely defined for each tuple `(I, T, A)` where `I` is a realized interface, `T` is a realized concrete type that implements `I`, and `A` is an associated type name on `I`. We can often resolve an associated type at compile time if the base type is fully realized. We can also utilize the interface-bounds on the associated type as well as information from unrealized generics (type constructors):

```baml
interface Foo {
	type Assoc extends Bar
}

interface Lorem<T> {
	type Asdf = T
}

class Ipsum<T> {
	implements Lorem<T> {}
}

function foo<T extends Foo>() -> void {
	let a: (int as Foo).Assoc; // fully realized at compile time
	let b: T.Assoc; // type variable that must implement `Bar`, realized a run time from `T`
	let c: (Ipsum<T> as Lorem<T>).Asdf.Assoc; // should be `T.Assoc`
}
```

### `Self`

`Self` is a [universal type](https://en.wikipedia.org/wiki/Parametric_polymorphism) which acts like a type alias for the concrete implementor type in implements blocks and interface declarations. The `self` method receiver always implicitly has the `Self` type.

For example:

```baml
interface Foo requires Add {
	/// Here, `Self` represents the implementor type.
	/// It can be thought of like a generic parameter that is bound to the
	/// concrete implementor type. Like generics, it has the bounds
	/// of the interface.
	function foo(self, a: Self) -> int {
		0
	}
}

implement Foo for int {
	/// Here we have `Self` bound to `int`. It no longer behaves like a
	/// generic parameter since we know exactly what type it uses.
	function foo(self, a: Self) -> int {
		self + a
	}
}
```

Normally when calling a method, we need to know what type all the generic parameters are at the call site. Similarly, we generally need to know what concrete `Self` type to use at the call site in order to dispatch to the correct implementation. The one exception to this is if the method has exactly one `Self`-typed parameter (including the `self` receiver): in this case, the caller can perform a dynamic dispatch treating `Self` as an interface-existential type since there is no chance of conflict or ambiguity.

The following illustrates why:

```baml
interface Foo {
	function one(self) -> int
	function two(self, other: Self) -> int
}
implement Foo for int {
	function one(self) -> int { self }
	function two(self, other: Self) -> int { self + other }
}
implement Foo for string {
	function one(self) -> int { 0 }
	function two(self, other: Self) -> int { -1 }
}

////////

function good() -> void {
	let a: int = 1;
	let b: int = 2;
	let c: Foo = 3; // erased to existential-type

	a.one(); // ok
	a.two(b); // ok: both `int`
	c.one(); // ok: dynamic dispatch
}
function bad() -> void {
	let a: int = 1;
	let b: string = "b";
	let c: Foo = a;
	let d: Foo = b;

	a.two(b); // bad: `Self` is `int` but we passed `string`
	c.two(d); // bad: both `Foo`-typed but possibly different concrete types;
	          //      we prohibit this at compile time.
}
```

There are a few different positions where `Self` might be used, which have different resolution paths:

1. In free functions and type aliases: compiler error. There is no type for `Self` to refer to
2. In class fields and methods: refers to the class type, accepting its type args.
3. In interface `implements` methods: refers to the `for`-target type, accepting its type args.
4. In `interface` default method bodies: the implementor for which the current call occurred. Since we do not expect the compiler to be able to fully monomorphize, this is typically not known until dynamically at call-time. As such, it is emitted as its own type arg.

## Functions

A function signature in BAML includes:

- Arguments (required, positional): a tuple of types
- Kwargs (optional, by-name): an unordered mapping of parameter names to types
- Return type: a single type. This is the only place that `void` is valid user-syntax.
- Error type: a single type.

All components of a function signature must be defined explicitly except for the error type. However, the error type must be unambiguously known:

1. Function declarations/signatures in interface declarations MUST declare a `throws` clause.
2. Function declarations with a `$rust_function`/`$rust_io_function`/`$compiler_intrinsic` MUST declare a `throws` clause
3. Other function and lambda declarations may omit a `throws` clause:
   - If declared, the compiler statically verifies that the body cannot throw an undeclared type.
   - If omitted, the compiler infers a `throws` clause from the content of the body.
   - A `throws` clause may also be _partial_: a `_` wildcard member (`throws AppError | _`) declares the named types **and** opens the remainder to inference. The body may then throw anything; the `_` is filled with the body's inferred throw set, and callers see the full union (declared names ∪ inferred). This is the precise counterpart of `throws unknown` — it keeps the named types in the contract and the inferred types exact, instead of widening the whole error type to `unknown`. A plain `throws T` (no `_`) stays exhaustive: every thrown type must be covered. Use `_` to avoid re-declaring infrastructural stdlib throws (e.g. `baml.json.*`) while still naming your own domain errors.
4. Function signature types used as function arguments or keyword arguments may omit a `throws` clause:
   - If declared, then the `throws` clause is used.
   - If omitted, then the compiler will add an implicit generic parameter to the surrounding function and use it as the function-typed-argument's `throws` clause:

```baml
function foo/*<E>*/(arg: () -> void /* throws E */) -> void /* throws E */ {
	arg()
}
```

   - An **optional** callback argument is a callback slot too, so it opens the
     same way: `arg: (() -> void)?` (and its longhand `(() -> void) | null`)
     gets the same implicit effect parameter, instantiated per call site.
     Passing `null` leaves it unconstrained, which defaults to `never`.

```baml
function bar/*<E>*/(arg: (() -> void /* throws E */)? = null) -> void /* throws E */ {
	if (arg != null) { arg() }
}
```

5. Otherwise, the `throws` clause MUST be declared. In particular, a function
   type nested any deeper than an argument's callback root — a list element, a
   map value, a class field, an alias body, a returned function type, or a
   callback's own parameter — is a stored/structural position with no single
   call site to instantiate an effect against, and must declare its `throws`.

As previously noted, BAML function declarations may include generics. However, all function calls and function-values must have their type parameters specified — or unambiguously inferable from context — to enable monomorphization at call-time:

```baml
function foo<T>(a: T) -> T { a }

////////

let a = foo; // INVALID: `foo` is not a valid function reference
let b = foo<int>; // ok: we have a valid realized type
let c: (int) -> int = foo; // ok: we can infer the type parameter
```

Since we can infer the type parameter(s), this should usually not be a major syntax burden on users: the most common use cases involve passing a function-value as a predicate or callback into another function, from which the types can be inferred. If, at a later date, we determine that there is value in permitting unrealized function types, we should be able to do so purely additively.

## Interfaces

Interfaces are contracts defining the behavior and/or shape of types. They are specified in [BEP-044](https://beps.boundaryml.com/beps/44). They include both fields and methods, but use Rust/Swift-like bounds rather than inheritance. Associated types are defined in [BEP-057](https://beps.boundaryml.com/beps/57). Only concrete types may implement interfaces. Blanket/bounds-based implementations simply apply an `implements` block to all concrete types satisfying the bounds. They are best thought of as paralleling Rust traits and much of their implementation is inspired by the structures used by Rust.

However, interfaces are (technically) not themselves types. Instead, each interface is used to create types:

- Existential types: like Rust's `dyn Trait` say that there exists _some_ type implementing the interface and which contains the value. BAML does not use the `dyn` keyword, so existential types in BAML _look_ like the interface itself is the type. This understanding is typically sufficient except in a few cases involving more complex type bounds.
- Universal types: in BAML as bounded generics, `T extends I` says that this code should work for _all_ types `T` that implement `I`.
  - `Self` on interface methods is also a universal type, not an existential type, as it refers to each concrete type, not the existential "`dyn`" type.
  - Arguably, unbounded generics are also universal types, just over the set of all BAML types instead of the (usually) smaller sets that are interfaces.

Coherence: As specified in BEP-044, each concrete type can have at most one implementation of a given interface. Like Rust's [orphan rule](https://doc.rust-lang.org/reference/items/implementations.html#r-items.impl.trait.orphan-rule), BAML requires that either the interface or the implementing type be defined in the current package to allow out-of-body `implements` blocks.

Like generic classes, generic interfaces must always have their type args explicitly specified unless in a position where they can unambiguously be inferred. If any generic args are specified, all non-defaulted generic args must be specified — except that an individual arg may be written as the `_` wildcard to infer just that position while keeping the rest explicit (e.g. `let fs: baml.future.Future<int, _>[] = [spawn { … }]` keeps the value type `int` explicit and infers the error type from the spawned body). A `_` is a real inference hole filled from context (the binding's initializer), not a synonym for the `unknown` top type: the filled-in type is exact, so it is not erased and downstream uses (such as `await`) see the precise type. `_` is only meaningful where the slot can be inferred from context (today: a `let` binding annotation and `throws`-clause members); it is not supported in positions with nothing to infer from, such as a bare function parameter or return type. Following Rust, in some places (e.g. whenever used as an interface-existential type), the associated types must also be specified. Naturally, type args/associated types with defaults may be omitted and will use said defaults.

While BAML tries to be permissive in where it will do unambiguous type inference, it is best practice to always explicitly parameterize all types.

Examples:

```baml
/// `baml.iter.Iterator` used as interface-existential must specify all.
/// Function signature return type never permits inference
function items() -> baml.iter.Iterator<Item=int> {
	// Type of `it` can be unambiguously inferred here,
	// so type args/associated types may be omitted
	let it: baml.iter.Iterator = [1, 2, 3].iter();
	it
}

/// Interface bounds require type args be specified
/// You can use a separate generic arg to make them generic:
function add<O, T extends baml.ops.Add<O>>(
	lhs: T,
	rhs: O,
) -> T.Output throws never {
	lhs + rhs
}

/// Interface bounds on generics do not require associated types:
function first<T extends baml.iter.Iterator>(it: T) -> T.Item? throws never {
	match (it.next()) {
		baml.iter.Done => null,
		let item: T.Item => item,
	}
}

/// However, they may specify them, similar to how type args must be specified
function first_int<T extends baml.iter.Iterator<Item=int>>(
	it: T
) -> int? throws never {
	match (it.next()) {
		baml.iter.Done => null,
		let item: int => item,
	}
}

/// These can be combined:
function add_makes_int<O, T extends baml.ops.Add<O, Output=int>>(
	lhs: T,
	rhs: O,
) -> int throws never {
	lhs + rhs
}
```

### Members of union-typed receivers

The member surface of a union-typed receiver is exactly the interface methods of the interfaces that _every_ member implements (at one shared instantiation), and nothing else. Inherent methods never participate: two classes each declaring their own `execute` give `A | B` no callable `execute`, even with identical signatures — only a shared interface that declares the member does. A call through the union then dispatches open-world on that shared interface, resolving against the runtime member's own implementation. This is effectively a call via upcasting to the shared interface-existential (thus requiring common associated-type pins) but provides a narrower callsite receiver type.

### Interface Implementations

Like in Rust, a fully realized concrete type `C` implements a fully realized interface `I` if and only if there exists some `implements` block where some generic instantiation produces `I` and `C` for its interface and "for" targets, respectively:

```baml
/// For all types `T`, `int` implements `Foo<T>`.
implements<T> Foo<T> for int {}
```

There is an alternate shorthand syntax which allows `implements` blocks inside of `class` definitions:

```baml
class Bar<T extends Bound> {
    implements Foo<T> {}
}
```

This is shorthand for the out-of-body `implements` block, and should be considered equivalent to

```baml
class Bar<T extends Bound> {}
/// transformed: takes the same generic args declaration and adds `for` that fills them all in.
implements<T extends Bound> Foo<T> for Bar<T> {}
```

The only substantive difference here is that the in-body class `implements` block form permits field links while the out-of-body variant does not. This is because only classes have fields, so restricting to in-body `implements` blocks gives us this check for free. However, at a later date it would not be unsound to permit field links in out-of-body `implements` blocks if and only if the `for` target provably only matches classes. Either way, the implementation should use a unified path for both forms beyond the syntax layers.

### Interface Coherence

BAML largely follows Rust's proven sound trait coherence rules to enforce that any given type has at most one implementation of any given interface. However, this is complicated in BAML by the fact that we have union types with algebraic equivalence: when searching over more complex pairs of `implements` blocks, the problem of determining whether they are disjoint is [NP-hard](https://arxiv.org/abs/1611.05672) in the general case. Fortunately, there are several common special-cases that we can solve more efficiently, and we can place a limit on the search space the compiler will attempt otherwise. When disjointness cannot be proven within that limit, the compiler conservatively rejects the `implements` blocks (it fails closed) rather than risk admitting an overlapping pair, so the at-most-one-implementation guarantee is never weakened by the search budget. In practice, this is unlikely to be a major obstacle to users, similar to TypeScript's type size limit.

The coherence problem asks the question: given two `implements` blocks, is there any overlap? That is, is there any realized type that both can apply an implementation of the same interface to?

The trivial (and most common) case is when the subject type is nominally distinct:

```baml
// ok: trivially no conflict
implement Foo for Bar { /* ... */ }
implement Foo for Lorem { /* ... */ }
```

We also have trivial conflict cases:

```baml
// compiler error: coherence violation
implement Foo for Lorem { /* ... */ }
implement Foo for Lorem { /* ... */ }

// compiler error: coherence violation
implement Foo for Lorem { /* ... */ }
implement<T> Foo for T { /* ... */ } // `T` can be `Lorem`
```

And some more involved but still simple cases:

```baml
// compiler error: coherence violation
implement<L> Foo for Pair<L, int> { /* ... */ }
implement<R> Foo for Pair<string, R> { /* ... */ } // overlap when `L = string` and `R = int`

// ok: always different first generic parameter
implement<R> Foo for Pair<int, R> { /* ... */ }
implement<R> Foo for Pair<string, R> { /* ... */ }

// compiler error: coherence violation
implement Foo for Bar<true | false> { /* ... */ }
implement Foo for Bar<bool> { /* ... */ } // `bool := true | false`

// ok: generics are invariant
implement Foo for Pair<int | string> { /* ... */ }
implement Foo for Pair<int> { /* ... */ } // `int` is not equivalent to `int | string`
```

However, it starts getting more complicated as we combine generic parameters and union equivalence:

```baml
// compiler error: coherence violation
implement<T> Foo for Bar<string | T> { /* ... */ }
implement Foo for Bar<string | Lorem> { /* ... */ } // overlap when `T = Lorem`

// compiler error: coherence violation
implement<T> Foo for Bar<(string | int) | T> { /* ... */ }
implement Foo for Bar<string | (int | Lorem)> { /* ... */ } // overlap when `T = Lorem`, unions (`|`) are associative

// compiler error: coherence violation
implement<T> Foo for Bar<string | T> { /* ... */ }
implement Foo for Bar<Lorem | string> { /* ... */ } // overlap when `T = Lorem`, unions (`|`) are commutative

// compiler error: coherence violation
implement<T> Foo for Bar<string | T> { /* ... */ }
implement Foo for Bar<string> { /* ... */ } // overlap when `T <: string`, unions (`|`) have idempotency
```

As seen above, we can get overlap stemming from equivalence over BAML union types, including associativity, commutativity, and idempotence. While these cases are still small enough to solve, one can imagine that determining for several different type variables whether there is any assignment that would produce an overlap has an exponentially large search space.

```baml
// Reduce 3-SAT to a coherence check. Encode the formula
//     (A ∨ B ∨ C) ∧ (¬A ∨ ¬B ∨ C)
// Each boolean variable is a type parameter the overlap search must resolve to
// `true` or `false`; each clause is a marker generic `ClauseI<...>`. The second block
// lists, per clause, every assignment that satisfies it (a 3-literal clause rules
// out exactly one of its 8 assignments, so 7 remain).

implement<A, B, C, Rest> Foo for Bar<
    Clause1<A, B, C> | Clause2<A, B, C> | Rest
> { /* ... */ }

implement Foo for Bar<
    // (A ∨ B ∨ C): all assignments except (false, false, false)
    Clause1<true,  true,  true > | Clause1<true,  true,  false> |
    Clause1<true,  false, true > | Clause1<true,  false, false> |
    Clause1<false, true,  true > | Clause1<false, true,  false> |
    Clause1<false, false, true > |
    // (¬A ∨ ¬B ∨ C): all assignments except (true, true, false)
    Clause2<true,  true,  true > | Clause2<true,  false, true > |
    Clause2<true,  false, false> | Clause2<false, true,  true > |
    Clause2<false, true,  false> | Clause2<false, false, true > |
    Clause2<false, false, false>
> { /* ... */ }
// Overlap exists iff some assignment of A, B, C satisfies both clauses
// (in this example `A=true`, `B=false`, `C=true`).
// `Rest` absorbs the satisfying assignments the two clauses don't pin,
// matching the cardinality of the second block. With n variables
// and m clauses, the witness search ranges over all 2^n assignments: 3-SAT.
```

As it turns out, the ACI (associativity-commutativity-idempotence) unification problem is NP hard [(Dudenhefner, et. al, 2017)](https://arxiv.org/abs/1611.05672). While we can trivially verify overlap given some witnessing assignment of type variables, the search space to find such a witness (or prove there is none) grows at least exponentially in the arbitrary case due to the ability of unions to reorder and simplify while maintaining equivalence.
