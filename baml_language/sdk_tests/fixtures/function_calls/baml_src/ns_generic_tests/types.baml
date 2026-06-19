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
//   crates/typescript_node/function_calls/customizable/generic_calls.test.ts

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

// A generic box carrying *methods* that read the class TypeVar back out via
// `reflect.type_of` (drives the "bound via caller" case): the host constructs
// a concrete `GenericBox<int>` and the method reports the bound type name. Also
// the fully-bound (`GenericBox<int>`) and outbound (`-> GenericBox<T>`) vehicle.
class GenericBox<T> {
  value T

  // Reflect the class-level binding the caller pinned at construction.
  function get(self) -> string {
    reflect.type_of<T>().to_string()
  }

  // Method-level TypeVar `U` fused with the class `T`: `U` is inferred from
  // the argument, then reflected alongside `T` as a union.
  function pair_with<U>(self, other: U) -> string {
    reflect.type_of<T | U>().to_string()
  }

  // Static constructor (no `self`): binds its OWN `V` via the caller's
  // `_types=`, with no class type args (the receiver doesn't exist yet). The
  // host sends only `{V}`. Constructs a `GenericBox<V>`. (The compiler requires
  // the static's type param to differ from the class param `T`, §179.)
  function new<V>(value: V) -> GenericBox<V> {
    GenericBox<V> { value: value }
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
    reflect.type_of<D | E>().to_string()
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

// ---------------------------------------------------------------------------
// Single TypeVar: infer the binding from a single argument value.
// ---------------------------------------------------------------------------

function identity<T>(x: T) -> T {
  x
}

// ---------------------------------------------------------------------------
// Return-position-only TypeVars with NO argument to infer from: the binding
// can only come from the host's explicit `_types=`. The cleanest proof the
// inbound path doesn't rely on argument inference. `T` (and `A`/`B`) appear
// solely in the body via `reflect.type_of`, never in a parameter.
// ---------------------------------------------------------------------------

function one_type_arg<T>() -> string {
  reflect.type_of<T>().to_string()
}

function two_type_args<A, B>() -> string {
  reflect.type_of<A | B>().to_string()
}

// ---------------------------------------------------------------------------
// Partially-bound generic-class param `GenericPair<int, T>`: `first` is pinned
// to `int`, so `T` binds from the second component only.
// ---------------------------------------------------------------------------

function second_of<T>(p: GenericPair<int, T>) -> T {
  p.second
}

// ---------------------------------------------------------------------------
// TypeVar fused into a union argument: `T` is fused into a union with concrete
// arms. The function returns `x` when it bound to `T`, else null.
//
// The intended body discriminates with a TypeVar match arm:
//     match (x) { let s: string => null, null => null, let v: T => v }
// but a bare-TypeVar arm is rejected today as an unreachable arm (type-tests
// on `T` fold to false — see baml-runtime-type-tests-limits). So this ships
// as a null-returning stub: it compiles, and the host tests stay xfailed
// until BOTH inbound inference and TypeVar match-arms land.
// ---------------------------------------------------------------------------

function tag_or_value<T>(x: T | string | null) -> T? {
  null
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
  reflect.type_of<A | B | C | D>().to_string()
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

function parse_as<T>(source: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
  baml.json.from_string<T>(source)
}

// ---------------------------------------------------------------------------
// Baseline: fully concrete instantiation, no reification needed. `T` is
// already bound at the definition site (`GenericBox<int>`).
// ---------------------------------------------------------------------------

function consume_int_wrapper(x: GenericBox<int>) -> int {
  x.value
}
