# baml_compiler2_rust

An ahead-of-time backend that turns BAML MIR into Rust source. It compiles
root functions and every function they transitively call into one Rust
module, together with a struct for every class and an enum for every enum
that code touches, and can
wrap the module in a standalone Cargo project with a host binary. Anything
outside the subset is rejected with a reason; nothing falls back to the VM.

The generated code links `bex_aot`, the native runtime, which takes the
language's semantics (checked integers, the float order, index resolution,
panics) from `bex_lang`, the crate the VM shares. This crate depends on
neither: it emits their paths as tokens.

```text
BAML --> ... --> MIR --+--> emit  --> bytecode --> bex_vm           (existing)
                       |
                       +--> baml_compiler2_rust --> Rust + bex_aot --> cargo --> binary
```

## The subset

A function is admitted when all of the following hold:

- it is non-generic and has no declared trace hook; it may be a method of a
  concrete class (declared in the class or an `implements` block), whose
  receiver is its first parameter; an interface's default method is not
  admitted; a defaulted parameter is admitted when its default is a
  constant (a literal, possibly negated, `null`, or an enum variant): the
  constant is passed from every call site that omits the argument, and the
  callee's prologue test against the omitted-argument sentinel is emitted
  as the constant `false`; the lambdas in its body are functions of their
  own, admitted with it, see [Function values](#function-values-and-closures);
- every parameter, local and the return place has a native type (below), or
  is a local whose declared type can be *refined* from its definitions
  (`unknown` and `baml.iter.Iterator<..>` temps of a for-in, the function-typed
  receiver temp lowering sometimes emits for `.length()`); `reflect.Type`
  locals may only hold a `load_type` feeding a generic builtin, and `never`
  may only be the destination of `baml.sys.panic`;
- its body uses `Assign`, `Drop`, `Nop`, `FreshCell` and trace-hook
  intrinsics (no-ops) over `Use`, `BinaryOp`, `UnaryOp`, `Array`, `Map`,
  `Len` of an array or a map, `Aggregate` of a class, `Discriminant` of an
  enum, `IsType` and `IsTypeTag` on a union, nullable or closed value (and
  the literal tests and the `baml.iter.Done` test), `TypeTag` of a union,
  `MakeClosure`, reading and writing locals, class fields, array elements
  (a map subscript lowers to a `baml.Map` call) and the cells of captured
  locals;
- its control flow uses `Goto`, `Branch`, `Switch` on `int` keys or on a
  union's type tag, `Return`, `Unreachable`, `ShortCircuit` (`&&`, `||`,
  `??`), `NarrowBind` on a union or nullable value, direct `Call`, `Call`
  through a function value, and the `VirtualCall`s of the for-in protocol,
  `sort` and `cmp`; `throw` of a class instance, and `catch` / `defer` in
  the shapes lowering gives them (unwind edges, landing blocks, `rethrow`,
  `throw_if_panic`, class tests, class bindings and class-tag switches on
  the caught value), see [Errors](#errors-throw-catch-defer) and
  [Unions](#unions-and-narrowing);
- every call is either a direct call of a source function in the subset, a
  call through a function value, a stdlib builtin from the table below
  (keyed by link name), or `baml.sys.panic` with a string literal; a source
  function used as a value is admitted like a callee.

Recursion is admitted: every function on a call cycle holds a
`bex_aot::depth::Guard` for its duration, so unbounded recursion throws
`baml.panics.StackOverflow` at the VM's frame limit instead of overflowing
the native stack. A cycle closed through a function value is not in the
static call graph, so every lambda and every function that calls through
a function value holds the guard too (every such cycle passes through one
of them). Functions off every cycle that call nothing indirectly pay
nothing.

### Value model

| BAML | Rust | notes |
|---|---|---|
| `int` | `Int63` | `+ - * / %` are `int::add(..)?` and friends, `<< >>` `int::shl/shr`, bitwise and comparisons direct; literals `int::lit(n)` |
| `bool` | `bool` | |
| `float` | `f64` | arithmetic native; `== != < <= > >=` through `float::eq` etc. (total order) |
| `bigint` | `BigInt` | `bex_aot::bigint::BigInt`, a counted pointer passed by reference: `+ - & \| ^` are `bigint::add(&a, &b)` and friends, `* / % << >>` the checked `bigint::mul(&a, &b)?` etc. (`AllocFailure` past the workspace cap, `DivisionByZero`, `NegativeBitShift`), comparisons `bigint::eq` / `bigint::cmp(..).is_lt()`; a mixed `int` operand is widened with `bigint::from_int`; literals `bigint::from_i64(n)` or `bigint::lit("digits")` |
| `string` | `Str` | `+` `string::concat`, `==` `string::eq`, `<` etc. `string::cmp(..).is_lt()`; literals `string::from_literal("..")` |
| `null`, `void` | `()` | |
| `T \| null` | `Option<T>` | `null` is `None`, a `T` stored into it is `Some(v)`, `x == null` is `x.is_none()` |
| `A \| B \| ..` | `Union_A_or_B` | a closed union (every member a known class, enum or primitive, or an array or map of those): a generated `pub enum Union_int_or_float { int(Int63), float(f64) }` with one variant per member in the order the type carries, `Copy` when every member is; `A \| B \| null` is `Option<Union_A_or_B>`; a member stored into it is its variant, `x is int` a `matches!`, see [Unions](#unions-and-narrowing) |
| `T[]` | `Shared<Vec<T>>` | `Shared<T> = Rc<RefCell<T>>`: reference semantics; `[a, b]` is `array::new::<T>(Vec::from([..]))`, `xs[i]` `array::get(&xs, i)?`, `xs[i] = v` `array::set(&xs, i, v)?`, `.length()` `array::len(&xs)` |
| class `C` | `Shared<user_C>` | generated `pub struct user_C { fields in declaration order }`; `C { .. }` is `shared(user_C { .., unspecified: None })`, `c.f` `c.borrow().f.clone()`, `c.f = v` `c.borrow_mut().f = v` |
| `map<K, V>` | `Map<K, V>` | `bex_aot::map::Map`: a `Shared` handle over an insertion-ordered table; `K` is `int`, `bool` or `string`; `{ k: v }` is `map::new::<K, V>(Vec::from([..]))`, `m[k]` `map::index(&m, &k)?` (`MapKeyNotFound` when absent), `m[k] = v` `map::set(&m, k, v)`, `.length()` `map::len(&m)` |
| enum `E` | `user_E` | generated fieldless `pub enum user_E { variants in declaration order }`, `Copy`; `E.V` is `user_E::V`, `==` compares variants, `match` switches on `int::lit(e as i64)` (the VM's discriminant), `to_string` and JSON use the variant's BAML name |
| `(A, B) -> R throws E` | `Rc<dyn Fn(A, B) -> Result<R, Thrown>>` | a counted pointer to a closure, shared by reference; the `throws` clause has no native form; `f(a, b)` is `(f)(a, b)?`; a function type with an optional parameter, a class field of function type, a union member of function type, `==`, `to_string` and JSON of a function value have no native form, see [Function values](#function-values-and-closures) |
| a captured local | `cell::Cell<T>` | `bex_aot::cell::Cell<T>`, a counted pointer to the value, held by the local's slot and cloned into every closure capturing it; `fresh_cell` is `cell::fresh()` / `cell::carry(&c)`, reads `cell::get(&c)?`, writes `cell::set(&c, v)` |
| for-in iterator | `bex_aot::array::Iter<T>` | refined from `virtual_call iter` on a `T[]` |
| result of `next` | `Option<T>` | refined; `is_type(x, Done)` is `x.is_none()`, the element copy `x.clone().expect(..)` |
| caught error | `Thrown` | a handler's error local: `bex_aot::Thrown`, a panic or a thrown class instance; its context local has no native form |

Literal types (`0`, `"x"`) map to their primitive, and a variant type
(`Color.Red`) to its enum, so a union of literals of one primitive
(`"a" | "b"`, `1 | 2 | int`) erases to that primitive and `"a" | "b" | null`
is `Option<Str>`; a `match` on a string literal is a `string::eq`. Classes
must be non-generic with every field in the model. Generated structs derive
`Serialize` and `Deserialize` through `bex_aot::serde` (`#[serde(rename)]`
keeps the BAML name when the Rust field had to change, e.g. `type` ->
`type_`) and implement `ToBaml`, rendering `Name { f: v, .. }` with the
unqualified class name. Every class and enum also implements
`bex_aot::Readable` (how the engine prints it inside an uncaught throw:
the fully qualified name, `user.Inner {n: 1}`), and every class
`bex_aot::ErrorClass` (its link name and readable fields), so any class can
be thrown and caught by name; a class that is never thrown pays nothing.

### Builtins

| MIR | Rust |
|---|---|
| `baml.Array.push<T>(ty, xs, v)` | `array::push(&xs, v); dest = array::len(&xs)` |
| `baml.json.deserialize<T>(ty, s)` | `json::deserialize::<T>(&s)?`, `T` from the `load_type` of `ty` |
| `baml.json.to_string(v)` | `json::to_string(&v)?` |
| `baml._to_string_default<T>(ty, v)` | `ToBaml::to_baml(&v)` |
| `baml.String.length` / `char_count` | `string::length(&s)` |
| `baml.String.is_ascii` | `string::is_ascii(&s)` |
| `baml.Float.floor` / `itrunc` | `float::floor(x)` / `float::itrunc(x)?` |
| `baml.ops.equals_equals(x, null)` | `x.is_none()` (`x: T \| null`) |
| `baml.ops.equals_equals(a, b)` on one enum | `a == b` |
| `baml.ops.equals_equals(a, b)` on primitives, nullables, unions of those | `bex_aot::eq::equals(&a, &b)` (`BamlEq`, the VM's broad `==`), the narrower side lifted into the wider type |
| `baml.Map.has` / `get` / `index` / `delete` (`m, k`) | `map::has(&m, &k)` / `map::get(&m, &k)` (`V \| null`) / `map::index(&m, &k)?` / `map::delete(&m, &k)` (the removed `V \| null`) |
| `baml.Map.set` / `get_or_insert` (`m, k, v`) | `map::set(&m, k, v)` (the previous `V \| null`) / `map::get_or_insert(&m, k, v)` |
| `baml.Bigint.abs` / `isqrt` / `to_int` / `parse` (`x`) | `bigint::abs(&x)` / `bigint::isqrt(&x)?` / `bigint::to_int(&x)?` / `bigint::parse(&s)?` |
| `baml.Bigint.pow` / `ilog` (`x, y`) | `bigint::pow(&x, &y)?` / `bigint::ilog(&x, &y)?` |
| `baml.Map.keys` / `values` / `length` / `clear` | `map::keys(&m)` / `map::values(&m)` (fresh arrays) / `map::len(&m)` / `map::clear(&m)` |
| `virtual_call iter as baml.iter.Iterable` on `T[]` | `array::iter(&xs)` |
| `virtual_call next as baml.iter.Iterator` on `Iter<T>` | `array::next(&mut it)` |
| `virtual_call sort as baml.Sortable` on `int[]`/`float[]`/`string[]` | `array::sort_int(&xs)` etc. |
| `virtual_call cmp as baml.ops.Compare` on `int`/`float`/`string`/`bigint`/`bool` | the language's order (`Ord::cmp`, `float::cmp`, `string::cmp`, `bigint::cmp`) as a `baml_ops_Ordering` variant |
| `baml.Array.map` / `filter` / `filter_map` / `for_each` / `some` / `every` / `find` / `find_index` / `find_last` / `find_last_index` / `flat_map` (`xs, f`), `reduce` (`xs, f, init`) | `array::map(&xs, &*f)?` etc., over a snapshot of the array, see [Function values](#function-values-and-closures) |
| `baml.Array.sort_by` (`xs, cmp`) / `sort_by_key` (`xs, key`) | `array::sort_by(&xs, &\|l, r\| ..)?` (the comparator's `Greater`) / `array::sort_by_key(&xs, &*key, &\|l, r\| <order>)?` in place, then `xs.clone()`; a `sort_by_key` key must be a primitive |
| `baml.sys.panic("..")` | `return Err(Thrown::from(Panic::UserPanic { .. }))` |

Any other stdlib function, with or without source, is rejected as
`` unsupported builtin `<link name>` ``, so the admission report names the next
builtin to add. A type argument that still mentions a type parameter rejects
the call.

### Function values and closures

A function type `(A, B) -> R throws E` is `Rc<dyn Fn(A, B) -> Result<R,
Thrown>>` ([`NativeTy::Fn`]): a counted pointer to a closure, cloned where
a handle would be, passed to a callee as a value. A call through one is
`(f)(a, b)?` ([`CallKind::Indirect`]); every argument must be given, as a
function type with an optional parameter has no native type. A declared
function used as a value (`let g = f`, `xs.map(f)`) is its function item
behind the pointer (`Rc::new(user_f)`) and is admitted like a callee; one
with a defaulted parameter is not, since the default is filled at direct
call sites only.

A lambda is a function of its own ([`FnId::Lambda`]), analyzed from the
`make_closure` that creates it once the enclosing function's locals are
typed, and emitted as `pub fn user_f__lambda0(..)` (nested:
`user_f__lambda0__lambda1`) beside its creator. The closure value is a
Rust closure calling it: `Rc::new(move |a| user_f__lambda0(&cell0, a))`.
Lambdas are never listed in `NativeModule::functions`; they have no
declaration.

A local a closure captures (`LocalDecl::is_captured`) holds a
`bex_aot::cell::Cell<T>` where the VM's slot holds an `Object::Cell`: a
counted pointer to the value, read and written through `Place::Deref`
(`cell::get(&c)?`, `cell::set(&c, v)`), cloned into every closure that
captures it, and lent to the lambda's function on every call as a leading
`_c0: &cell::Cell<T>` parameter (a nested lambda forwards `_c0` by cloning
it). A captured parameter is celled on entry with the value the caller
passed (`let mut _1: cell::Cell<T> = cell::with(_1);`), as the VM's frame
preamble cells it. `fresh_cell(_n)` replaces the cell (`cell::fresh()`,
or `cell::carry(&_n)` for a C-style `for` header binding copied into the
next iteration), which is what gives a binding declared in a loop body a
cell per iteration: closures created in different iterations see different
cells, and closures created in one iteration, or anywhere else over one
binding, see one value. Mutation through a capture is therefore visible to
the enclosing scope and vice versa, a closure returned from its creator
keeps its cells alive, and a cell read as a call argument copies the value
out before the call, so no borrow is live across it. A read of a cell no
store has reached is the `Unreachable` panic (the checker should have
rejected the program; the VM would read `null`).

The `baml.Array` methods that call back into a function value map to
`bex_aot::array` functions that take the callback as a `&dyn Fn`. Every one
walks a snapshot of the array taken before the first callback, as the VM's
continuations do, so a callback that pushes to the array is not observed by
the walk and no `RefCell` borrow is held while a callback runs. `sort_by`
is the VM's bottom-up merge sort over the snapshot, comparison for
comparison (a comparator with side effects sees the same sequence of
calls), written back only once every comparison has returned: a throwing
comparator leaves the array as it was, a comparator that reads the array
sees it unsorted, and one that mutates it has the changes overwritten.
`sort_by_key` computes every key once, left to right, then orders by the
key's primitive order with the same stable sort. A callback whose type is
not the one the runtime calls is adapted at the call site when every
argument stores into its parameter and its result stores into what the
runtime reads: `&move |a| { let value = f(Some(a))?; Ok(value) }`, with
a `filter_map` callback that never returns `null` wrapped in `Some` and a
`for_each` callback's result discarded. The result type of `map`,
`filter_map`, `flat_map`, `reduce` and the key type of `sort_by_key` come
from the call's type arguments, so `(int | null)[]` from a `map` whose
callback returns `int` is a `Wrap` in the adapter. `cmp` on a primitive is
the language's order as a `baml.ops.Ordering` variant, which is what a
comparator usually returns.

A function value has no rendering (`to_string`), no JSON form, no `==`
(the VM compares closures by identity and pools declared functions, which
a fresh `Rc` per use would not reproduce), and cannot be a class field or
a union member: the generated struct and enum render, serialize and
compare their members. `(T) -> R | null` is `Option<Rc<..>>` as any
nullable. A method used as a value (`obj.m` without a call,
`make_bound_method`) is rejected with that reason: the corpus has 19 such
functions. A function value of one type stored where a wider function type
is expected (a callback returning `int` passed as `(int) -> int | null`
outside the array methods) is rejected: the VM passes the value as it is,
and native code has no adapter there yet.

### Unions and narrowing

A closed union is a generated Rust enum ([`NativeTy::Union`]), one per
distinct member list the program mentions, with `ToBaml`, `Readable`,
`Serialize` and `Deserialize` delegating to the member held, and `BamlEq`
when every member compares natively. The member order is the one the type
carries: the type system canonicalizes signatures and locals (a `float |
int` parameter is `int | float`), but a `load_type` template
(`deserialize<Ok | Err>`) and a class field keep their spelled order, and
the VM decodes JSON in that order (the first member the text decodes as),
so `Ok | Err` and `Err | Ok` are two native types, converted at a store by
a `match` that re-tags each variant. The same conversion widens a union
into one with more members. JSON decoding is `bex_aot::json::deserialize_union`
over the variants in order; the error is a `DecodeError` as on the VM.

Lowering reads a union as one of its members wherever the checker narrowed
it (after `x is int`, in the arms of a `match`, after `x != null` on a
nullable union), with the union-typed local as it is. The analyzer admits
such a read as a `Coercion::Narrow` (`Unwrap` for a nullable, `UnwrapNarrow`
for both) in a plain copy, a binary or unary operand (read as the other
operand's type, or the result's when both sides are narrowed), a call or
builtin argument, an array element or a class field; the printer emits
`match x { Union::int(value) => value, _ => <raise Unreachable> }`, so a
narrowing the checker got wrong is a panic, never a miscompile. A field read
through a narrowed union local (`v.n` after `v is A`) is rejected: the slot
belongs to the narrowed class, which the MIR does not name; the binding
form (`let a: A => a.n`) is admitted.

Type tests are resolved once by the analyzer to the variants they select
(`Candidate::member_tests`) and printed as `matches!`: `is_type(x, int)` is
`matches!(&x, Union::int(_))`, `x is int | float` lists both, a class or
enum test names its variant, a literal test adds a guard
(`Union::int(value) if *value == int::lit(1)`), `is_type_tag` (lowering's
coarse container test) selects every variant carrying the tag, and on a
nullable the patterns are wrapped in `Some(..)` with the `null` test
`is_none()`. A binding pattern (`let n: int => ..`) lowers to a
`narrow_bind` on a temporary seeded with the union; it prints as the same
test, the binding being the value itself read narrowed afterwards. Four or
more type arms lower to `type_tag` and a `Switch` with the VM's tag numbers
and class keys; the tag local is never declared and the switch prints as a
`match` on the union value with one pattern per selected variant. A type
test on a value of one closed type (`x is int` on an `int`, `xs is int[]`)
is the constant the static type decides, and `c is Color.Red` on an enum
is `c == Color::Red`.

`==` and `!=` with a union or nullable operand, other than against `null`,
lift the other side into the wider type and compare through
`bex_aot::BamlEq` (false across kinds, `float` by the reflexive order, enums
by variant), which is what the VM's comparison opcode decides for the
values it holds and what a variant pattern on a union (`Color.Red =>` on an
`int | Color`) lowers to. `baml.ops.equals_equals` on two such values is
the same comparison. `a ?? b` is `if let Some(value) = a { .. } else {
<b> }`.

### Errors: throw, catch, defer

Every generated function returns `Result<T, Thrown>`, so a thrown value is
an `Err`. Lowering fixes, per basic block, the handler a throw or panic
anywhere in the block lands in (`BasicBlock::unwind`: the enclosing `catch`
handler or `defer` landing pad), and a handler block *lands* with the thrown
value in its error local (`Landing::error_local`). The backend keeps that
shape:

- `throw v` is `Err(Thrown::error(v))` for a class instance (the handle is
  boxed as the object, so a `catch` binding sees the same object), or
  `Err(e)` for a caught error thrown on; `rethrow` carries the caught
  error as it is. In a block with no handler either is a `return`.
- A block with a handler reaches it with a `break`: the structurer treats
  the unwind edge as an ordinary edge (every graph including them is
  reducible), labels the handler like a merge node, and records the jump
  (`Structured::unwind_jumps`); the printer wraps every fallible
  expression in the block (`int::div`, `array::get`, a callee's call, ..)
  as `match .. { Ok(v) => v, Err(e) => { _err = Thrown::from(e); break
  'bbN; } }` instead of `?`, and a `throw`, `rethrow`, panic or
  unreachable exit as the same assignment and jump. The MIR's paths are the
  output's paths, so rustc's definite-initialization check still proves
  that the handler reads an assigned error local.
- A `catch` arm's class test (`is_type(e, C)`) is
  `bex_aot::thrown::is_class(&e, "<link name>")`, decided by name as the VM
  decides by class identity (two classes never share a name); a class
  binding (`let x: C => ..`, a `narrow_bind` into a temporary seeded with
  the error) is `if let Some(v) = thrown::downcast::<Shared<C>>(&e)`; four
  or more class arms switch on the error's class tag
  (`type_tag(e)`, a `&'static str` local holding the name) with a `match`
  on string literals. The `throw_if_panic` guard lowering puts in front of a
  wildcard arm is `if thrown::is_panic(&e) { <rethrow> }`, where a panic is
  a `baml.panics.*` instance by class, as on the VM, so a program-built
  `baml.panics.Exit { code }` is a panic too (and exits with its code when
  uncaught). `catch_all_panics` has no guard.
- `defer` needs nothing of its own: lowering copies the body at every
  non-throwing exit (fall-through, `return`, `break`, `continue`), in LIFO
  order, and lands the unwinding path in a pad that runs the body and
  `rethrow`s, chaining to the next pad. A throw inside a pad replaces the
  in-flight error, as on the VM. The `shielded` mark on a defer body (no
  cancellation delivered while it runs) has nothing to act on: native code
  has no cancellation yet (no async).
- The `baml.errors.Context` a handler lands with beside the error (stack
  trace, cause chain) is not materialized: `rethrow` and `throw_if_panic`
  carry the error alone, and a read of a bound context (`catch (e, ctx)`)
  is rejected.

The parity contract with the VM is the thrown class, its non-message
fields and the exit code (`Thrown::exit_code`); message text agrees only
where the code producing it is shared (`bex_lang`).

## Emission scheme

Every MIR local is a `let mut _N: T;` at the top of the function, declared
without an initializer. The structured output has exactly the paths the MIR
has, so rustc's definite-initialization check proves that every read follows
a write on every path, over the same graph a backend pass would; a MIR that
reads an unassigned local surfaces as an `E0381` in the generated crate.
Parameters are `mut _1..=_arity`, the result is `_0`, and the function
returns `Result<T, bex_aot::Thrown>`.

Reads of a handle or string clone it (`Rc` / rope clones); array element
reads are owned values from `array::get`. A temporary that only copies a
handle so a later statement can read through it (`_8 = copy _1; _7 =
_8[_9]`, which lowering emits for every subscript and field read) is an
*alias* of its source: the printer reads `array::get(&_1, _9)` and never
declares `_8`. The analyzer (`function::find_aliases`) allows this only when
the temporary is assigned once, mentioned only after that assignment in the
same block, and nothing reassigns the source in between; the alias is an
`Rc` increment and decrement saved on every element read, which is most of
the cost of an array loop.

A store into a field or element computes the value in a block first
(`{ let value = ..; base.borrow_mut().f = value; }`), so a `borrow()` an
operand took on the same cell is released before the `borrow_mut()`. Lowering
evaluates call arguments into locals before the call; admission checks that
(a non-local argument is `Rejection::Invalid`), so no borrow is live across a
call.

The analyzer records how each assignment's value fits its destination
(`Candidate::stores`: identity, `Some(..)` for a `T` into `T | null`, `None`
for a `null` into one, a variant for a member into a union, a re-tagging
`match` between unions, a narrowing `match` for a union read as a member)
and the value's own type (`Candidate::values`), and the printer applies
them; the printer never re-derives an rvalue's type.

Control flow is structured, never a `loop { match block { .. } }` dispatcher.
`structure.rs` implements the Stackifier / "Beyond Relooper" scheme over a
reducible CFG:

- blocks are emitted in reverse postorder inside their immediate dominator's
  region;
- a merge node (two or more forward in-edges) opens a labeled block `'bbN: {}`
  at its dominator, which its predecessors leave with `break 'bbN`; its own
  code follows the block;
- a loop header becomes `'bbN: loop {}`; back edges are `continue 'bbN`;
- the lowest-numbered exit a loop header dominates follows the `loop` and is
  reached with `break 'bbN`; other merging exits open their labeled blocks
  around the loop;
- a `Switch` is a `match d.get() { 1i64 | 2i64 => { .. } _ => { .. } }`, with
  arms sharing a target grouped, and an arm that shares the default's target
  folded into `_`, so every reachable block appears exactly once.

Reducible graphs need no block duplication and no state variable. A retreating
edge whose target does not dominate its source is reported as
`Rejection::Invalid("irreducible control flow")`: BAML lowering only produces
single-entry loops, so that is a compiler bug.

Tokens are built with `quote!`, parsed back with `syn` and printed with
`prettyplease`; a parse failure is reported as `Rejection::Invalid`. An exit
in tail position (the end of the function, through `if` and `match` arms) is
the function's value, `Ok(_0)`; every other exit is a `return`. The module
starts with a `//!` header and an `#![allow(..)]` naming the lints structured
MIR output trips: unused `mut`, variables, assignments, labels and imports,
unreachable code, the `_N` local and `user_C` struct and function names, the
`return`s a panic or a dead write leaves inside a leaf block, and the
expressions a program may legitimately write (`y = y`, `0.0 / 0.0`).

## Invariants of the MIR this relies on

Neither is checked by `verify_mir` today; both are checked here, so a MIR
change that breaks one is reported as `Rejection::Invalid`, never
miscompiled.

- **Reducibility.** Lowering is syntax-directed with single-header loops, no
  labeled jumps, and `defer` bodies replayed inline; no pass threads jumps
  or clones blocks. Measured over 15,012 freshly lowered functions (the
  corpus, the stdlib from source, benchmark and stress cases, at O0/O1/O2,
  unwind edges included): 613 with cycles, 0 irreducible.
- **Call arguments are locals or constants.** Operands are evaluated into
  temporaries before a call, which is what keeps a `RefCell` borrow from
  being live across one.

When the stdlib changes, a renamed builtin degrades to `Unsupported`; a
changed arity or result type of a mapped builtin is `Invalid`, so the table
above is the place to update. The corpus sweep (`baml_tests/tests/
native_corpus.rs`) compiles every admitted function of the conformance corpus
on every build and, by hand, runs `rustc` over the result.

## Using it

```rust
use baml_compiler2_rust::{compile_many, write_project, ProjectOptions};

let module = compile_many(&db, &[prepare, run])?;   // roots + transitive callees
std::fs::write("lib.rs", &module.rust_source)?;
write_project(&module, out_dir, &ProjectOptions {
    crate_name: "my-program",
    runtime_path: Path::new("/path/to/crates/bex_aot"),
    release_profile: true,
})?;
```

`admit(&db, loc)` answers for one function without looking at its callees
(the classes it mentions are checked); `admit_closure(&db, loc)` answers for
the function together with everything it calls, which is what `compile`
needs, and is what `baml __emit-rust --report` and `--all` count.
`compile_many(&db, &roots)` compiles several roots (and their shared
callees, once each) into one module; `compile` is that with one root.
`NativeModule::roots` indexes the roots in request order, `entry` is the
first of them, `functions` carry each function's native parameter and return
types (`NativeTy`), and `classes` lists the generated structs.

Generated names are a pure function of the link name, `rust_name`: every
character other than an ASCII letter or digit becomes `_`, so
`user.sum_of_squares` is `user_sum_of_squares`, the method `user.C.get` is
`user_C_get` and the class `user.Cell` is the struct `user_Cell`;
`rust_name_for(&db, loc)` does the same from a `FunctionLoc`. Two link names
that sanitize alike are rejected rather than renamed.

`write_project` writes `Cargo.toml` (its own `[workspace]`, `bex_aot` by
path as the only dependency since serde is re-exported from it, and with
`release_profile` an `opt-level = 3`, fat-LTO, single codegen unit,
`panic = "abort"` release profile), `src/lib.rs`, `src/main.rs` and
`mir.txt`. The host shim is std-only: it parses `--<param> <value>` flags in
any order (`int` via `str::parse::<i64>` then `bex_aot::int::check`, `bool`
via `true|false`, `float` via `str::parse::<f64>`, `string` as the raw
argument), prints usage for `--help`, reports bad or missing arguments as
`error: ...` on stderr with exit code 1, prints the result through `ToBaml`
on success, and on an uncaught throw prints `error: uncaught throw:
<rendered>` and exits with `Thrown::exit_code`. When the entry function takes
a class, array or nullable parameter (`CompiledFunction::shim_callable` is
false, and `admit` reports it through `Admitted::shim_callable`), writing the
project still succeeds: `main` prints `` error: entry `user.x` takes
non-scalar arguments; link the library instead `` and exits with code 2.
`baml __emit-rust` takes `--function` repeatedly (the first is the shim's
entry) or `--all` for every admitted function (the shim is built around the
first one it can call), and marks library-only functions in its report.

## Rejections

`Rejection::Unsupported(reason)` means the function is outside the subset.
Types: a map keyed by anything but `int`, `bool` or `string` (keys of
those types compare by value on both backends; a `float` key's NaN and a
class's own `Hash`/`Equals` are not reproduced), an open union (a member
that is `unknown`, an interface, a generic class, a type alias, a function
or media: the reason names the member, `interface (a union member)`),
`uint8array`, media and future types, a function type with an optional
parameter, a class field of function type, interfaces, generic classes, a
class with such a field (the field is named), and an `unknown` or
interface-typed local no definition refines. A `to_string` on a class or
enum with its own `baml.ToString` implementation is rejected, as the
structural rendering would be wrong; so is `==` on an enum, or a union with
one, that implements its own `baml.ops.Equals`, which the VM dispatches.
Constructs: `throw` of a value that is not a class instance (`throw
"text"`, which BAML allows), a `catch` arm that is not a class test, a class
binding or a wildcard (`let s: string => ..`), a binding of a generic class,
of a class outside the model, or of a stdlib error or panic class (`let p:
baml.panics.IndexOutOfBounds => ..`, see Limitations), a read of the bound
context of `catch (e, ctx)`, `spawn`/`await`, sys-ops, a field read
through a narrowed union (`v.n` after `v is A`; bind it instead), a type
test against a float or bigint literal, `==` on arrays, maps, classes,
function values or a union with such a member (the VM compares class
instances structurally, with the class's own `Equals` when it has one; the
runtime does not), `to_string` or JSON of a function value, a method used
as a value, a stdlib or generic function used as a value, a function with
a defaulted parameter used as a value, a lambda in a generic function, a
function value stored where a wider function type is expected, an omitted
argument in a call through a function value, a `sort_by_key` key or a
`cmp` receiver that is not a primitive, an array callback whose type the
runtime cannot call, a JSON decode into a type that mentions a literal type
(the VM rejects a value outside the literal, which the erased primitive
would accept; the field is named), interface method calls other than
`iter`/`next`/`sort`/`cmp`, `sort` on a non-primitive array, calls with
trace attachments, an omitted argument to a stdlib function, or type
arguments to a user function, any stdlib function not in the table, a
`panic` with a computed message, a defaulted parameter whose default is
computed (`b: int = a + 1`), interface default methods, generic functions
and declared trace hooks.

`Rejection::Invalid(reason)` means the MIR violated an invariant a checked
program must hold (a compiler bug): lowering errors, type mismatches between
an assignment and its place, between an array literal and its elements, or
between a call and its callee, a call argument that is not a local, an
irreducible CFG, unknown block ids, or generated tokens that do not parse.

Two MIR quirks are handled deliberately: the fall-through edge of
`while (true) { .. }` assigns `null` to a non-nullable return place, which
the checker has proven dead, so the assignment is emitted as
`return Err(Thrown::from(Panic::Unreachable))` rather than rejected; and a
`baml.sys.panic` in tail position of a `void` function has the return place
as its destination, which it never stores to.

## Limitations

- `Array.push` returns the array's length, so the emitted call re-reads it
  into the (usually unused) destination temp.
- `Rvalue::TraceHookSettings` and stdlib BAML-source bodies are not compiled;
  the stdlib functions the benchmarks need are mapped directly, including
  `for_each`, `filter_map` and `sort_by_key`, whose BAML bodies the VM runs.
- A `catch` binding of a stdlib error or panic class
  (`let e: baml.panics.IndexOutOfBounds => e.index`) is rejected: the
  runtime raises those as types of its own (`bex_lang::Panic`,
  `bex_aot::errors`, `bex_aot::json`), which the generated struct of the
  class would never downcast to. The class test
  (`baml.panics.IndexOutOfBounds => ..`) is native.
- The parity contract with the VM is values, control flow, the class and
  non-message fields of a thrown object, and the exit code. The text of a
  message is identical only where the code producing it is shared
  (`bex_lang`); a `catch` that reads `e.message` observes each backend's
  own wording otherwise.
- A union spelled in two member orders is two native types (see
  [Unions](#unions-and-narrowing)): the VM's JSON decode is order-sensitive
  per site, and the conversion between the two is a re-tagging `match` at
  every store across them. One type with per-site decoders is the
  alternative, not taken yet.
- A union's class members compare structurally on the VM; natively `==` on
  such a union is rejected, as `==` on a class is.
- A cycle through a closure (a closure stored in a cell it captures, or in
  a field of an object it captures) is never freed, like a cycle through
  class fields: handles are counted, not collected (D2). A closure that
  captures nothing, or only values the cycle does not pass through, is
  freed with its last pointer.
- A function value never crosses a function-type boundary with an adapter
  outside the array methods: `(int) -> int` passed as `(int) -> int | null`
  is rejected rather than wrapped.
