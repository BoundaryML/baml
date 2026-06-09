This doc is primarily written for use in development work on the BAML programming language, but may also be useful to users of the language.

At the time of this document's writing, not all BAML type system features work as specified here. This document is thus prescriptive, not descriptive: this is how the type system *should* work. Any features or implementations that rely on preexisting incomplete behavior should be considered liable to change.

# The BAML Type System
The BAML programming language has a strong static type system. While it uses TypeScript-like syntax, it provides stronger guarantees and identity which also permit safe runtime validation (e.g. `match` over union types).

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
   If there were not invariant, the following case would cause the type contract to be invalid:
   ```baml
   // THIS CODE IS FOR DEMONSTRATIVE PURPOSES; IT IS INVALID BAML
   let a: int[] = [1, 2, 3];
   let b: (int | string)[] = a; // if covariant, this would be valid
   b[1] = "hello"; // The type `int | string` allows `string`
   let c: int = b[1]; // oops!!!!! it's actually a `string`
   ```
   This is a major point in which TypeScript fails: it would permit the above unsound code, resulting in `c` having a potentially cascading type/state mismatch.
2. Union members are covariant: all members are subtypes of the union (as are their subtypes).
	1. Same with interfaces and `unknown`
	2. Literal types behave as union members within concrete types
3. Function types:
	1. Arguments must be contravariant (function A is subtype only if its arguments are supertype)
		- This includes optional arguments: function A is subtype only if its set of optional arguments are a superset and the type of each of them is a supertype.
	2. Return and error types must be covariant (function A is subtype only if its arguments are subtype)
	3. Example:
	   ```baml
	   function foo(a: int | string) -> bool throws never { 0 }
	   
	   //////// EXAMPLE CALLSITE ////////
	   /// args of `foo` are more permissive to caller so it is valid:
	   /// return type of `foo` is less expectant of caller so it is valid:
	   let a: (int) -> bool | float throws never = foo;
	   ```

### BAML Subtyping Cases
Literals are subtypes which together comprise their concrete types:
- `(int.min_value() | ... | -2 | -1 | 0 | 1 | 2 | ... | int.max_value()) =: int`
	- Note that the extrema are not valid BAML type syntax, one would have to write out the full literal.
- `(true | false) =: bool`
- `("" | "\\x00" | "\\x01" | ... | "all strings" | ...) =: string` (infinite size set)
- `(... | -2n | -1n | 0n | 1n | 2n | ...) =: bigint` (infinite size set)
- `(Enum.VariantA | Enum.VariantB | ...) =: Enum` (for each enum type)

Other subtypes:
- bottom type: `never <: T` for all `T`
- top type: `T <: unknown` for all `T`
- unions: `T <: (T | ...)` for all `T`
    - Including union subsets: `(T0 | T1) <: (T0 | T1 | ...)`
    - Also including optionals: `T? =: T | null`
- identity: `T =: T` for all `T`
- interface bounds: `A <: B` for interfaces `A` and `B` where `A requires B` (for interface-existential types)
- interface membership: `C <: I` for concrete type `C` that implements interface `I` (for interface-existential type)
- functions: `((a0, a1, ..., ao0=..., ...) -> ar throws ae) <: ((b0, b1, ..., bo0=..., ...) -> br throws be)` if and only if:
    - Same number of positional arguments where each `bN <: aN` (contravariant)
    - Each optional argument ([BEP-033](https://beps.boundaryml.com/beps/33)) `boN` has a corresponding `aoN` of the same name where `boN <: aoN` (contravariant, subset function type has superset of optional args)
    - Return type: `ar <: br` (covariant)
    - Error type: `ae <: be` (covariant)

In nearly all contexts, we can thus simplify types and maintain equivalence: `1 | int == int | 1 == int`, `anything | unknown == unknown`, `true | false == bool`, `Side.Left | Side.Right == Side` (enum), etc. The only exceptions are places like LLM function return types where the order affects the parsing. However, this should only matter in this one location based on explicit declaration for the SAP parser, and can still be treated the same for in-language type algebra.

## Pattern Matching
Pattern matching ([BEP-015](https://beps.boundaryml.com/beps/15)) on types falls naturally out of BAML's subset-based subtyping system. For any given value, BAML can determine whether a value is a member of a given type. A `match` expression can be thought of as checking each arm in order to see if the value is a member of the type-set (though various optimizations allow us to implement this more efficiently in many cases).

To check pattern fallibility/infallibility/exhaustiveness, BAML ensures that the union of the matched patterns represent a superset of the scrutinee's type, or defines a diverging path for fallible patterns if not. `match` also rejects cases that can provably never be reached. While these do not necessarily violate the type system, they are dead code that could be misleading.

## Function Calls and Generics
Whenever a function is called, its generic parameters must be known at that time. Generally, this is achieved via static typing: we pass or infer generics for the function call. This enables correct dynamic dispatch for interface methods and means that within each function call, it behaves as if it were monomorphized (though the implementation may not necessarily be). This means that after being bound at the start of the call, generic parameters cannot change for the given function call. The bound type for a generic parameter must be a subtype of the `extends` bounded type, if any.

This should generally be enforced by the compiler within BAML code, but requires careful handling at boundaries with Rust code or when interacting across FFI with a host language.

### Reflection
The reason we need to know the generics at the callsite is that, unlike TypeScript, types are reflectable at run-time and have substantive impact on the program's execution. For example (see [BEP-39](https://beps.boundaryml.com/beps/39) for more info on reflection):
```baml
function is_int<T>() -> bool {
	reflect.type_of<T>() == reflect.type_of<int>()
}
```

Like Rust, dynamic dispatch can also affect this. As an example (should work but may not yet depending on when you are reading this):
```baml
function positiveish<T extends AdditiveIdentity & baml.ops.Compare>(
	value: T
) -> bool {
	value > T.add_identity()
}
```

While we can sometimes optimize this away at compile time, there are many cases (such as calls from bridges/rust/eval-like code) where the VM must explicitly handle realized generics.

## Interfaces
Interfaces are contracts defining the behavior and/or shape of types. They are specified in [BEP-044](https://beps.boundaryml.com/beps/44). They include both fields and methods, but use Rust/Swift-like bounds rather than inheritance. Associated types are defined in [BEP-057](https://beps.boundaryml.com/beps/57). Only concrete types may implement interfaces. Blanket/bounds-based implementations simply apply an `implements` block to all concrete types satisfying the bounds.

However, interfaces are (technically) not themselves types. Instead, each interface is used to create types:
- Existential types: like Rust's `dyn Trait` say that there exists *some* type implementing the interface and which contains the value. BAML does not use the `dyn` keyword, so existential types in BAML *look* like the interface itself is the type. This understanding is typically sufficient except in a few cases involving more complex type bounds.
- Universal types: in BAML as bounded generics, `T extends I` says that this code should work for *all* types `T` that implement `I`.
	- `Self` on interface methods is also a universal type, not an existential type, as it refers to each concrete type, not the existential "`dyn`" type.
	- Arguably, unbounded generics are also universal types, just over the set of all BAML types instead of the (usually) smaller sets that are interfaces.

Coherence: As specified in BEP-044, each concrete type can have at most one implementation of a given interface. Like Rust's [orphan rule](https://doc.rust-lang.org/reference/items/implementations.html#r-items.impl.trait.orphan-rule), BAML requires that either the interface or the implementing type be defined in the current package to allow out-of-body `implements` blocks.

Like generic classes, generic interfaces must always have their type args specified unless in a position where they can unambiguously be inferred. If any generic args are specified, all non-defaulted generic args must be specified (at a later date we may add `_` syntax to infer individual args). Following Rust, in some places (e.g. whenever used as an interface-existential type), the associated types must also be specified. Naturally, type args/associated types with defaults may be omitted and will use said defaults.

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
		let item: T => item,
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
