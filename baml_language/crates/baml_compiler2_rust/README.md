# baml_compiler2_rust

An ahead-of-time backend that turns BAML MIR into Rust source. It compiles
root functions and every function they transitively call into one Rust
module, together with a struct for every class that code touches, and can
wrap the module in a standalone Cargo project with a host binary.

The generated code links the `bex_lang` crate (`crates/bex_lang`) for the
value semantics both backends must agree on: checked integers, strings,
floats, shared arrays and class instances, JSON, `to_string` rendering and
thrown values. This crate never depends on `bex_lang`; it only emits its
paths.

## The subset

A function is admitted when all of the following hold:

- it is a free, non-generic function with no defaulted parameters, no declared
  trace hook and no lambdas in its body;
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
  `baml.sys.panic` with a string literal;
- the admitted call graph is acyclic.

### Value model

| BAML | Rust | notes |
|---|---|---|
| `int` | `Int63` | `+ - * / %` are `int::add(..)?` and friends, `<< >>` `int::shl/shr`, bitwise and comparisons direct |
| `bool` | `bool` | |
| `float` | `f64` | arithmetic native; `== != < <= > >=` through `bex_lang::float` (total order) |
| `string` | `Str` | `+` `string::concat`, `==` `string::eq`, `<` etc. `string::cmp(..).is_lt()`; literals `string::from_literal("..")` |
| `null`, `void` | `()` | |
| `T \| null` | `Option<T>` | the only union shape; `null` is `None`, a `T` stored into it is `Some(v)`, `x == null` is `x.is_none()` |
| `T[]` | `Shared<Vec<T>>` | `Shared<T> = Rc<RefCell<T>>`: reference semantics; `[a, b]` is `array::new::<T>(Vec::from([..]))`, `xs[i]` `array::get(&xs, i)?`, `xs[i] = v` `array::set(&xs, i, v)?`, `.length()` `array::len(&xs)` |
| class `C` | `Shared<user_C>` | generated `pub struct user_C { fields in declaration order }`; `C { .. }` is `shared(user_C { .., unspecified: None })`, `c.f` `c.borrow().f.clone()`, `c.f = v` `c.borrow_mut().f = v` |
| for-in iterator | `bex_lang::array::Iter<T>` | refined from `virtual_call iter` on a `T[]` |
| result of `next` | `Option<T>` | refined; `is_type(x, Done)` is `x.is_none()`, the element copy `x.clone().expect(..)` |

Literal types (`0`, `"x"`) map to their primitive. Classes must be
non-generic with every field in the model; a class whose method is called is
rejected because the method is. Generated structs derive `Serialize` and
`Deserialize` through `bex_lang::serde` (`#[serde(rename)]` keeps the BAML
name when the Rust field had to change, e.g. `type` -> `type_`; `f64` fields
serialize NaN and infinities as `null`) and implement `ToBaml`, rendering
`Name { f: v, .. }` with the unqualified class name.

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

Every MIR local is a `let mut` at the top of the function: scalars
zero-initialized (`Int63::ZERO`, `false`, `0.0`, `()`), heap types declared
without a value. A definite-initialization analysis rejects any read a path
reaches before an assignment (array index and field writes count their base
and index locals as reads), so neither a zero nor Rust's own check ever
stands in for a missing BAML value. Parameters are `mut _1..=_arity`, the
result is `_0`, and the function returns `Result<T, bex_lang::Thrown>`.

Reads of a handle or string clone it (`Rc` / rope clones); array element
reads are owned values from `array::get`. A store into a field or element
computes the value in a block first (`{ let value = ..; base.borrow_mut().f =
value; }`), so a `borrow()` an operand took on the same cell is released
before the `borrow_mut()`. MIR evaluates operands into temps before calls, so
no borrow is live across a call.

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
  arms sharing a target grouped so no block is duplicated.

Reducible graphs need no block duplication and no state variable. A retreating
edge whose target does not dominate its source is reported as
`Rejection::Invalid("irreducible control flow")`: BAML lowering only produces
single-entry loops, so that is a compiler bug.

Tokens are built with `quote!`, parsed back with `syn` and printed with
`prettyplease`; a parse failure is reported as `Rejection::Invalid`, never a
panic. An exit in tail position (the end of the function, through `if` and
`match` arms) is the function's value, `Ok(_0)`; every other exit is a
`return`. The module starts with a `//!` header and an `#![allow(..)]` naming
the lints structured MIR output trips: unused `mut`, variables, assignments,
labels and imports, unreachable code, the `_N` local and `user_C` struct
names, and the `return`s a panic or a dead write leaves inside a leaf block.

## Using it

```rust
use baml_compiler2_rust::{compile_many, write_project, ProjectOptions};

let module = compile_many(&db, &[prepare, run])?;   // roots + transitive callees
std::fs::write("lib.rs", &module.rust_source)?;
write_project(&module, out_dir, &ProjectOptions {
    crate_name: "my-program",
    runtime_path: Path::new("/path/to/crates/bex_lang"),
    release_profile: true,
})?;
```

`admit(&db, loc)` answers for one function without looking at its callees
(the classes it mentions are checked). `compile_many(&db, &roots)` compiles
several roots (and their shared callees, once each) into one module;
`compile` is that with one root. `NativeModule::roots` indexes the roots in
request order, `entry` is the first of them, `functions` carry each
function's native parameter and return types (`NativeTy`), and `classes`
lists the generated structs.

Generated names are a pure function of the link name, `rust_name`: every
character other than an ASCII letter or digit becomes `_`, so
`user.sum_of_squares` is `user_sum_of_squares` and the class `user.Cell` is
the struct `user_Cell`; `rust_name_for(&db, loc)` does the same from a
`FunctionLoc`. Two link names that sanitize alike are rejected rather than
renamed.

`write_project` writes `Cargo.toml` (its own `[workspace]`, `bex_lang` by
path as the only dependency since serde is re-exported from it, and with
`release_profile` an `opt-level = 3`, fat-LTO, single codegen unit release
profile), `src/lib.rs`, `src/main.rs` and `mir.txt`. The host shim is std-only: it parses `--<param> <value>` flags in
any order (`int` via `str::parse::<i64>` then `bex_lang::int::check`, `bool`
via `true|false`, `float` via `str::parse::<f64>`, `string` as the raw
argument), prints usage for `--help`, reports bad or missing arguments as
`error: ...` on stderr with exit code 1, prints the result through `ToBaml`
on success, and on an uncaught throw prints `error: uncaught throw:
<rendered>` and exits with `Thrown::exit_code`. When the entry function takes
a class, array or nullable parameter (`CompiledFunction::shim_callable` is
false, and `admit` reports it through `Admitted::shim_callable`), writing the
project still succeeds: `main` prints `` error: entry `user.x` takes
non-scalar arguments; link the library instead `` and exits with code 2, and
the harness glue calls the library's `user_prepare` / `user_run` directly.
`baml __emit-rust` takes `--function` repeatedly (the first is the shim's
entry) or `--all` for every admitted function, and marks library-only
functions in its report.

## Rejections

`Rejection::Unsupported(reason)` means the function is outside the subset and
the bytecode stays authoritative for it. Types: enums, maps, unions other than
`T | null`, `bigint`, `uint8array`, media, function and future types,
interfaces, generic classes, a class with such a field (the field is named),
and an `unknown` or interface-typed local no definition refines. Constructs:
`catch`/`defer` (any block with an unwind, landing, handling or shield),
`throw`, `spawn`/`await`, sys-ops, closures and captured locals, `??`,
narrowing patterns, map literals and indexing, interface method calls other
than `iter`/`next`/`sort`, `sort` on a non-primitive array, indirect calls,
calls with trace attachments, named or omitted arguments, or type arguments
to a user function, any stdlib function not in the table, a `panic` with a
computed message, `int` literals outside the 63-bit range, defaulted
parameters, methods, generic functions, declared trace hooks, and recursion
anywhere in the admitted call graph.

`Rejection::Invalid(reason)` means the MIR violated an invariant a checked
program must hold (a compiler bug): lowering errors, a read before definite
assignment, type mismatches between an assignment and its place, between an
array literal and its elements, or between a call and its callee, an
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
- Only plain `f64` fields get the NaN/infinity -> `null` serializer;
  `float[]` and `float | null` fields serialize through `serde_json`'s
  default, which also writes `null` for non-finite values.
- Error classes for `throw`/`catch` are not generated (v3).
