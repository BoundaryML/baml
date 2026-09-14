// Generic *function-call* coverage for the bridge-generics2 project: the
// host -> engine call path for generic functions whose TypeVars must be
// bound from the call. Companion to type_shapes/ns_generics, which covers
// generic *class shapes* and concrete (`<int>`) round trips; this namespace
// covers generic *functions* (binding `T` by argument inference, binding
// multiple TypeVars at once, unifying `T` across argument positions,
// partially-bound and nested generic-class params, caller-bound generic-class
// methods, and the return-position-only case that needs an explicit type
// argument).
//
// Design + case ordering:
//   thoughts/sam-projects/bridge-generics/00a-design.md
//
// Host tests:
//   crates/python_pydantic2/function_calls/customizable/test_generic_calls.py
//   crates/typescript/function_calls/customizable/generic_calls.test.ts

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

// A generic box carrying *methods* that read the class TypeVar back out via
// `reflect.Type.of` (drives the "bound via caller" case): the host constructs
// a concrete `GenericBox<int>` and the method reports the bound type name. Also
// the fully-bound (`GenericBox<int>`) and outbound (`-> GenericBox<T>`) vehicle.
class GenericBox<T> {
  value T

  // Reflect the class-level binding the caller pinned at construction.
  function get(self) -> string {
    reflect.Type.of<T>().to_string()
  }

  // Method-level TypeVar `U` fused with the class `T`: `U` is inferred from
  // the argument, then reflected alongside `T` as a union.
  function pair_with<U>(self, other: U) -> string {
    reflect.Type.of<T | U>().to_string()
  }

  // Static constructor (no `self`): binds its OWN `V` via the caller's
  // `_types=`, with no class type args (the receiver doesn't exist yet). The
  // host sends only `{V}`. Constructs a `GenericBox<V>`. (The compiler requires
  // the static's type param to differ from the class param `T`, §179.)
  function new<V>(value: V) -> GenericBox<V> {
    GenericBox<V> { value: value }
  }

  // A receiver-free method may still reference an enclosing class TypeVar.
  // Go exposes that binding as an inferred helper type parameter and sends it
  // as a named structured type argument.
  function static_echo(value: T) -> T {
    value
  }

  function static_pair<U>(left: T, right: U) -> string {
    reflect.Type.of<T | U>().to_string()
  }

  // Python stub regressions: required value inference must coexist with a
  // defaulted keyword-only argument on both instance and static methods.
  function pair_with_default<U>(self, other: U, label: string = "default") -> U {
    other
  }

  function new_with_default<V>(value: V, label: string = "default") -> GenericBox<V> {
    GenericBox<V> { value: value }
  }

  // No argument can carry V, so host stubs must require `_types=` while still
  // rendering a valid zero-argument static-method signature.
  function static_type_name<V>() -> string {
    reflect.Type.of<V>().to_string()
  }
}

// Static method whose TypeVar names differ from the enclosing class's params
// (class `<A, B, C>`, static `make<D, E>`). Proves the named `TyArg` wire slots
// each binding by name into the static frame's own params (`[D, E]`), with no
// phantom class params — the case the named wire exists for (01pt5).
class NamedStatic<A, B, C> {
  first A
  second B
  third C

  function make<D, E>(d: D, e: E) -> string {
    reflect.Type.of<D | E>().to_string()
  }
}

class GenericPair<A, B> {
  first A
  second B
}

class GenericRecursive<T> {
  value T
  next GenericRecursive<T>?
}

// One TypeVar reused across every field-shape position: a bare `T`, a list, a
// map value, an optional, and a union arm.
class ContainerShapes<T> {
  item T
  items T[]
  by_key map<string, T>
  maybe T?
  mixed T | string | null
}

// A concrete user class to bind `T` to in the inference/return cases.
class StringIntPair {
  my_string string
  my_int int
}

// Multiple TypeVars on one class, each in a different field shape (drives the
// multi-TypeVar free-function case): a bare `A`, a list of `B`, a map of `C`.
class GenericTriple<A, B, C> {
  first A
  second B[]
  third map<string, C>
}

// Go projection adversaries: the class and method/free-function TypeVars
// normalize to the same exported identifier and must still share one
// collision-safe helper/signature scope without changing their wire names.
class GenericNameCollision<foo_bar> {
  value foo_bar

  function pair<fooBar>(self, other: fooBar) -> string {
    reflect.Type.of<foo_bar | fooBar>().to_string()
  }

  function make<fooBar>(value: fooBar) -> GenericNameCollision<fooBar> {
    GenericNameCollision<fooBar> { value: value }
  }
}

function generic_name_collision<foo_bar, fooBar>(left: foo_bar, right: fooBar) -> string {
  reflect.Type.of<foo_bar | fooBar>().to_string()
}

// ---------------------------------------------------------------------------
// Single TypeVar: infer the binding from a single argument value.
// ---------------------------------------------------------------------------

function identity<T>(x: T) -> T {
  x
}

// A required value carries T even though a later argument is defaulted.
function identity_with_default<T>(x: T, label: string = "default") -> T {
  x
}

// Defaulted parameters remain runtime inference positions: when omitted,
// Rule 4 binds T to rust_type, so Python stubs may omit `_types=`.
function optional_only<T>(x: T? = null) -> T? {
  x
}

// ---------------------------------------------------------------------------
// Return-position-only TypeVars with NO argument to infer from: the binding
// can only come from the host's explicit `_types=`. The cleanest proof the
// inbound path doesn't rely on argument inference. `T` (and `A`/`B`) appear
// solely in the body via `reflect.Type.of`, never in a parameter.
// ---------------------------------------------------------------------------

function one_type_arg<T>() -> string {
  reflect.Type.of<T>().to_string()
}

function two_type_args<A, B>() -> string {
  reflect.Type.of<A | B>().to_string()
}

// ---------------------------------------------------------------------------
// Partially-bound generic-class param `GenericPair<int, T>`: `first` is pinned
// to `int`, so `T` binds from the second component only.
// ---------------------------------------------------------------------------

function second_of<T>(p: GenericPair<int, T>) -> T {
  p.second
}

// ---------------------------------------------------------------------------
// TypeVar fused into a union argument: `T` appears as an arm of the parameter's
// union (`T | string | null`). The binding for `T` can only come from the
// caller's explicit `_types=`; the body reflects it back as a string so the
// host can assert which type `T` was bound to.
// ---------------------------------------------------------------------------

function tag_or_value<T>(x: T | string | null) -> string {
  reflect.Type.of<T>().to_string()
}

// ---------------------------------------------------------------------------
// Recursive generic-class argument: bind `T` from the element type of a
// `GenericRecursive<T>`. Returns the head value (not a real sum — arithmetic
// isn't available for an unconstrained `T`); the test pins TypeVar binding +
// decode of the element, not summation.
// ---------------------------------------------------------------------------

function list_head<T>(list: GenericRecursive<T>) -> T {
  list.value
}

// ---------------------------------------------------------------------------
// Unification: the same `T` appears in two argument positions. The two call
// sites must unify to one binding (and disagreeing inputs force the host to
// specify `T` explicitly).
// ---------------------------------------------------------------------------

function choose<T>(left: T, right: T) -> T {
  if (true) { left } else { right }
}

// ---------------------------------------------------------------------------
// One `T` across many field positions of a single argument; the binding is
// read back out of the list field.
// ---------------------------------------------------------------------------

function read_items<T>(shape: ContainerShapes<T>) -> T[] {
  shape.items
}

// ---------------------------------------------------------------------------
// Multiple TypeVars on a free function: `A`, `B`, `C` are each bound from a
// differently-shaped argument (scalar, list, map) and reassembled into a
// `GenericTriple<A, B, C>`. Exercises binding more than one TypeVar at once.
// ---------------------------------------------------------------------------

function make_triple<A, B, C>(a: A, b: B[], c: map<string, C>) -> GenericTriple<A, B, C> {
  GenericTriple<A, B, C> { first: a, second: b, third: c }
}

// ---------------------------------------------------------------------------
// Nested generic: the argument is a `GenericPair<GenericPair<A, B>,
// GenericPair<C, D>>`, so all four TypeVars are recovered by walking the
// nested instantiation. Reflects the union of the four bindings.
// ---------------------------------------------------------------------------

function extract<A, B, C, D>(a: GenericPair<GenericPair<A, B>, GenericPair<C, D>>) -> string {
  reflect.Type.of<A | B | C | D>().to_string()
}

// ---------------------------------------------------------------------------
// Outbound inference (`foo<T>(arg0: T) -> GenericBox<T>`): infer `T` from the
// argument, then return a generic instance constructed over it.
// ---------------------------------------------------------------------------

function wrap<T>(x: T) -> GenericBox<T> {
  GenericBox<T> { value: x }
}

// ---------------------------------------------------------------------------
// `T` appears in return position only — there's no argument to infer it from,
// so the host must bind it explicitly (Python `_types=`, TS `$types`).
// Mirrors the real `fetch_as<T>` shape: forwards `T` into `from_string<T>`.
// ---------------------------------------------------------------------------

function parse_as<T>(source: string) -> T throws baml.json.ParseError | baml.json.DecodeError {
  baml.json.from_string<T>(source)
}

// ---------------------------------------------------------------------------
// Baseline: fully concrete instantiation, no reification needed. `T` is
// already bound at the definition site (`GenericBox<int>`).
// ---------------------------------------------------------------------------

function consume_int_wrapper(x: GenericBox<int>) -> int {
  x.value
}

// ---------------------------------------------------------------------------
// Non-generic functions returning REIFIED generics: the return type pins the
// class type args at the *definition* site, so there's no caller binding and no
// inference — the host just has to decode a fully-concrete generic instance
// flowing OUTBOUND. The outbound mirror of `consume_int_wrapper` (concrete
// generic inbound). Four reification shapes the host decoder must handle:
//   * one TypeVar used once           -> GenericBox<int>
//   * one TypeVar used many times     -> ContainerShapes<int> (bare/list/map/opt/union)
//   * a nested generic type arg       -> GenericBox<GenericBox<int>>
//   * multiple TypeVars, mixed fields -> GenericTriple<int, string, bool>
//                                        (scalar int, string list, bool-valued map)
// ---------------------------------------------------------------------------

// One TypeVar, used once.
function make_int_box() -> GenericBox<int> {
  GenericBox<int> { value: 7 }
}

// One TypeVar (`int`), reified into every field shape at once.
function make_int_container() -> ContainerShapes<int> {
  ContainerShapes<int> {
    item: 1,
    items: [1, 2, 3],
    by_key: { "k": 4 },
    maybe: null,
    mixed: 5
  }
}

// The type arg is itself a generic instance: GenericBox<GenericBox<int>>.
function make_nested_box() -> GenericBox<GenericBox<int>> {
  GenericBox<GenericBox<int>> { value: GenericBox<int> { value: 9 } }
}

// Multiple TypeVars over mixed field shapes: a scalar int, a string list, and a
// bool-valued map.
function make_int_str_bool_triple() -> GenericTriple<int, string, bool> {
  GenericTriple<int, string, bool> {
    first: 1,
    second: ["a", "b"],
    third: { "k": true }
  }
}

// ---------------------------------------------------------------------------
// §J variance soundness (02d/02e): a `TypeVar` that occurs at more than one
// position whose variances conflict has no consistent binding and the call must
// REJECT, instead of fabricating an unsound union. The invariant-container
// shapes below (`T[]`, `map<_, T>`, `GenericBox<T>`) are reachable from inbound
// values; the contravariant function-parameter cases live at the unifier layer
// (a host callable crosses the FFI as an opaque handle, not a structural
// function type — see 02d / J13). Each body just reflects `T` so the regression
// (agreeing-occurrence) twins can read the bound type back.
// ---------------------------------------------------------------------------

// Two invariant list-element occurrences: pair(int[], string[]) rejects;
// pair(int[], int[]) binds T = int.
function pair<T>(a: T[], b: T[]) -> string {
  reflect.Type.of<T>().to_string()
}

// Two invariant map-value occurrences: conflicting value types reject.
function merge<T>(a: map<string, T>, b: map<string, T>) -> string {
  reflect.Type.of<T>().to_string()
}

// Two invariant generic-class-arg occurrences: GenericBox<int> vs GenericBox<string> rejects.
function combine<T>(x: GenericBox<T>, y: GenericBox<T>) -> string {
  reflect.Type.of<T>().to_string()
}

// Invariant (`arr: T[]`) × covariant (`bare: T`) in one call: glue(int, string[])
// rejects; glue(int, int[]) binds T = int.
function glue<T>(bare: T, arr: T[]) -> string {
  reflect.Type.of<T>().to_string()
}

// §D n-ary covariant join: three covariant bare-arg occurrences union-merge.
function triple_choose<T>(a: T, b: T, c: T) -> string {
  reflect.Type.of<T>().to_string()
}

// §J multiple unconstrained TypeVars in one union: un-inferrable ⇒ reject (two
// free vars beside a concrete `int` member have no principled split).
function two_in_union<T, U>(x: T | U | int) -> string {
  reflect.Type.of<T | U>().to_string()
}

// The ambiguous union cannot bind either free var by itself, but separate
// ordinary value positions can bind both before the engine's fallback gate.
function two_in_union_with_values<T, U>(x: T | U | int, left: T, right: U) -> string {
  reflect.Type.of<T | U>().to_string()
}

// ---------------------------------------------------------------------------
// §I nullable param: the non-null arm of `T?` binds against a present actual
// (`maybe_id(5)` ⇒ T=int), while a `null`-only actual is no evidence ⇒ T falls
// to host-only `rust_type` (we do NOT null-strip `T?` to bind `T=null`). Returns
// the value so the host can assert the round trip.
// ---------------------------------------------------------------------------

function maybe_id<T>(x: T?) -> T? {
  x
}

// ---------------------------------------------------------------------------
// §B low-evidence free functions: a free function has no wire-arg channel, so an
// EMPTY collection yields no element/value evidence and the element/value
// TypeVar falls to host-only `rust_type` (B7/B9). The bodies return the empty
// container so the host reads back `[]` / `None` rather than a reflected name.
// (Contrast the *bound instance* path, where an empty field still recovers `T`
// from the wire type-arg — B6.)
// ---------------------------------------------------------------------------

function first_or<T>(xs: T[]) -> T? {
  if (xs.length() > 0) { xs[0] } else { null }
}

// §B/§D heterogeneous array: the elements of a single `T[]` union-merge while
// synthesizing the one container's element type, so `elem_type([1, "x"])` binds
// `T = int | string`. Reflects `T` so the host can read the unified element type
// directly (distinct from B8, which only reads back the round-tripped values).
function elem_type<T>(xs: T[]) -> string {
  reflect.Type.of<T>().to_string()
}

function values_of<T>(m: map<string, T>) -> T[] {
  m.values()
}

// ---------------------------------------------------------------------------
// §J J13 bridge-level reification of function-typed args: a host callable
// crosses the FFI as an opaque handle, not a structural `(T) -> R`, so the
// bridge can neither infer `T`/`R` from it nor validate a binding inferred
// elsewhere against it. Every TypeVar TOUCHED by the callable is therefore
// poisoned and must be specified up front — `T` is poisoned by `(T)` even though
// `x` would otherwise pin it, and `R` lives only in the closure's return. The
// body invokes the callable so the explicit-binding positive case round-trips.
// ---------------------------------------------------------------------------

function apply<T, R>(f: (T) -> R, x: T) -> R {
  f(x)
}

// ---------------------------------------------------------------------------
// §I enum actual: an enum value binds `T` to the enum TYPE (`identity(SomeEnum.VARIANT)`
// ⇒ T=SomeEnum), recovered from the resolved variant.
// ---------------------------------------------------------------------------

enum SomeEnum {
  VARIANT
  OTHER
}
