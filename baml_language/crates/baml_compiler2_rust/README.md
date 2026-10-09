# baml_compiler2_rust

An ahead-of-time backend that turns BAML MIR into Rust source. It compiles
root functions and every function they transitively call into one Rust
module, together with a struct for every class that code touches, and can
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

- it is non-generic, has no declared trace hook and no lambdas in its body;
  it may be a method of a concrete class (declared in the class or an
  `implements` block), whose receiver is its first parameter; an interface's
  default method is not admitted; a defaulted parameter is admitted when its
  default is a constant (a literal, possibly negated, `null`, or an enum
  variant): the constant is passed from every call site that omits the
  argument, and the callee's prologue test against the omitted-argument
  sentinel is emitted as the constant `false`;
- every parameter, local and the return place has a native type (below), or
  is a local whose declared type can be *refined* from its definitions
  (`unknown` and `baml.iter.Iterator<..>` temps of a for-in, the function-typed
  receiver temp lowering sometimes emits for `.length()`); `reflect.Type`
  locals may only hold a `load_type` feeding a generic builtin, and `never`
  may only be the destination of `baml.sys.panic`;
- its body uses `Assign`, `Drop`, `Nop` and trace-hook intrinsics (no-ops) over
  `Use`, `BinaryOp`, `UnaryOp`, `Array`, `Len`, `Aggregate` of a class,
  literal `IsType` and the `baml.iter.Done` test, reading and writing
  locals, class fields and array elements;
- its control flow uses `Goto`, `Branch`, `Switch` on `int` keys, `Return`,
  `Unreachable`, `ShortCircuit` (`&&`, `||`), direct `Call` and the
  `VirtualCall`s of the for-in protocol and `sort`;
- every call is either a direct call of a source function in the subset, a
  stdlib builtin from the table below (keyed by link name), or
  `baml.sys.panic` with a string literal.

Recursion is admitted: every function on a call cycle holds a
`bex_aot::depth::Guard` for its duration, so unbounded recursion throws
`baml.panics.StackOverflow` at the VM's frame limit instead of overflowing
the native stack. Functions off every cycle pay nothing.

### Value model

| BAML | Rust | notes |
|---|---|---|
| `int` | `Int63` | `+ - * / %` are `int::add(..)?` and friends, `<< >>` `int::shl/shr`, bitwise and comparisons direct; literals `int::lit(n)` |
| `bool` | `bool` | |
| `float` | `f64` | arithmetic native; `== != < <= > >=` through `float::eq` etc. (total order) |
| `string` | `Str` | `+` `string::concat`, `==` `string::eq`, `<` etc. `string::cmp(..).is_lt()`; literals `string::from_literal("..")` |
| `null`, `void` | `()` | |
| `T \| null` | `Option<T>` | the only union shape; `null` is `None`, a `T` stored into it is `Some(v)`, `x == null` is `x.is_none()` |
| `T[]` | `Shared<Vec<T>>` | `Shared<T> = Rc<RefCell<T>>`: reference semantics; `[a, b]` is `array::new::<T>(Vec::from([..]))`, `xs[i]` `array::get(&xs, i)?`, `xs[i] = v` `array::set(&xs, i, v)?`, `.length()` `array::len(&xs)` |
| class `C` | `Shared<user_C>` | generated `pub struct user_C { fields in declaration order }`; `C { .. }` is `shared(user_C { .., unspecified: None })`, `c.f` `c.borrow().f.clone()`, `c.f = v` `c.borrow_mut().f = v` |
| for-in iterator | `bex_aot::array::Iter<T>` | refined from `virtual_call iter` on a `T[]` |
| result of `next` | `Option<T>` | refined; `is_type(x, Done)` is `x.is_none()`, the element copy `x.clone().expect(..)` |

Literal types (`0`, `"x"`) map to their primitive. Classes must be
non-generic with every field in the model. Generated structs derive
`Serialize` and `Deserialize` through `bex_aot::serde` (`#[serde(rename)]`
keeps the BAML name when the Rust field had to change, e.g. `type` ->
`type_`) and implement `ToBaml`, rendering `Name { f: v, .. }` with the
unqualified class name.

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
| `virtual_call iter as baml.iter.Iterable` on `T[]` | `array::iter(&xs)` |
| `virtual_call next as baml.iter.Iterator` on `Iter<T>` | `array::next(&mut it)` |
| `virtual_call sort as baml.Sortable` on `int[]`/`float[]`/`string[]` | `array::sort_int(&xs)` etc. |
| `baml.sys.panic("..")` | `return Err(Thrown::from(Panic::UserPanic { .. }))` |

Any other stdlib function, with or without source, is rejected as
`` unsupported builtin `<link name>` ``, so the admission report names the next
builtin to add. A type argument that still mentions a type parameter rejects
the call.

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
for a `null` into one) and the printer applies it; the printer never
re-derives an rvalue's type.

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
Types: enums, maps, unions other than `T | null`, `bigint`, `uint8array`,
media, function and future types, interfaces, generic classes, a class with
such a field (the field is named), and an `unknown` or interface-typed local
no definition refines. Constructs: `catch`/`defer` (any block with an unwind,
landing, handling or shield), `throw`, `spawn`/`await`, sys-ops, closures and
captured locals, `??`, narrowing patterns and values the checker narrowed
(a `T | null` used as a `T` after a null test), map literals and indexing,
interface method calls other than `iter`/`next`/`sort`, `sort` on a
non-primitive array, indirect calls, calls with trace attachments, an
omitted argument to a stdlib function, or type arguments to a user function,
any stdlib function not in the table, a `panic` with a computed message, a
defaulted parameter whose default is computed (`b: int = a + 1`), interface
default methods, generic functions and declared trace hooks.

`Rejection::Invalid(reason)` means the MIR violated an invariant a checked
program must hold (a compiler bug): lowering errors, type mismatches between
an assignment and its place, between an array literal and its elements, or
between a call and its callee, a call argument that is not a local, an
irreducible CFG, unknown block ids, or generated tokens that do not parse.

One MIR quirk is handled deliberately: the fall-through edge of
`while (true) { .. }` assigns `null` to a non-nullable return place. The
checker has proven that edge dead, so the assignment is emitted as
`return Err(Thrown::from(Panic::Unreachable))` rather than rejected.

## Limitations

- `Array.push` returns the array's length, so the emitted call re-reads it
  into the (usually unused) destination temp.
- `Rvalue::TraceHookSettings` and stdlib BAML-source bodies are not compiled;
  the stdlib functions the benchmarks need are mapped directly.
- Error classes for `throw`/`catch` are not generated.
- The parity contract with the VM is values, control flow, the class and
  non-message fields of a thrown object, and the exit code. The text of a
  message is identical only where the code producing it is shared
  (`bex_lang`); `catch` is not admitted, so no native program observes a
  message.
