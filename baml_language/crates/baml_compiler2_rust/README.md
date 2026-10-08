# baml_compiler2_rust

An ahead-of-time backend that turns BAML MIR into Rust source. It compiles an
entry function and every function it transitively calls into one Rust module,
and can wrap that module in a standalone Cargo project with a host binary.

The generated code links the `bex_lang` crate (`crates/bex_lang`) for the
integer semantics and panic payloads both backends must agree on. This crate
never depends on `bex_lang`; it only emits its paths.

## The subset

A function is admitted when all of the following hold:

- it is a free, non-generic function with no defaulted parameters, no declared
  trace hook and no lambdas in its body;
- every local, parameter and return place is `int` or `bool`; the return place
  and call destinations may also be `void`/`null` (emitted as `()`), and the
  destination of a `baml.sys.panic` call may be `never`;
- its body uses only `Assign`, `Drop` and `Nop` statements over `Use`,
  `BinaryOp`, `UnaryOp` and literal `IsType` rvalues (how `match` on a `bool`
  or `int` literal lowers);
- its control flow uses only `Goto`, `Branch`, `Switch` on `int` keys,
  `Return`, `Unreachable`, `ShortCircuit` (`&&`, `||`) and direct `Call`;
- every call names a source function positionally, with no type arguments,
  trace attachment or unwind edge; the one exception is `baml.sys.panic` with a
  string literal, which is emitted as `return Err(Panic::UserPanic { .. })`;
- the admitted call graph is acyclic.

Overflow, division by zero and negative shifts are runtime checks: `+ - * / %`
become `bex_lang::int::add(a, b)?` and friends, `<< >>` go through
`int::shl`/`int::shr`, the bitwise operators and comparisons are direct.

## Emission scheme

Every MIR local is a `let mut` at the top of the function, zero-initialized
(`Int63::ZERO`, `false`, `()`); a definite-initialization analysis rejects any
read a path reaches before an assignment, so the zero never stands in for a
missing BAML value. Parameters are `mut _1..=_arity`, the result is `_0`, and
the function returns `Result<T, bex_lang::Panic>`.

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
panic. The module starts with a `//!` header and a blanket `#![allow(..)]` for
the lints structured MIR output trips (unused `mut`, labels, unreachable code).

## Using it

```rust
use baml_compiler2_rust::{compile, write_project, ProjectOptions};

let module = compile(&db, entry_loc)?;          // entry + transitive callees
std::fs::write("lib.rs", &module.rust_source)?;
write_project(&module, out_dir, &ProjectOptions {
    crate_name: "my-program",
    runtime_path: Path::new("/path/to/crates/bex_lang"),
    release_profile: true,
})?;
```

`admit(&db, loc)` answers for one function without looking at its callees.
`compile_many(&db, &roots)` compiles several roots (and their shared callees,
once each) into one module; `compile` is that with one root. `NativeModule::roots`
indexes the roots in request order and `entry` is the first of them.

Generated names are a pure function of the link name, `rust_name`: every
character other than an ASCII letter or digit becomes `_`, so
`user.sum_of_squares` is `user_sum_of_squares`; `rust_name_for(&db, loc)` does
the same from a `FunctionLoc`. Two link names that sanitize alike are rejected
rather than renamed.

`write_project` writes `Cargo.toml` (its own `[workspace]`, `bex_lang` by
path, and with `release_profile` an `opt-level = 3`, fat-LTO, single codegen
unit release profile), `src/lib.rs`, `src/main.rs` and `mir.txt`. The host
shim is std-only: it parses `--<param> <value>` flags in any order (`int` via
`str::parse::<i64>` then `bex_lang::int::check`, `bool` via `true|false`),
prints usage for `--help`, reports bad or missing arguments as `error: ...` on
stderr with exit code 1, prints the result on success, and on an uncaught
panic prints `error: uncaught throw: <rendered panic>` and exits with the
panic's exit code.

## Rejections

`Rejection::Unsupported(reason)` means the function is outside the subset and
the bytecode stays authoritative for it: other types (`string`, `float`,
arrays, classes, ...), `catch`/`defer` (any block with an unwind, landing,
handling or shield), closures and captured locals, intrinsics and trace hooks,
generics and type arguments, defaulted parameters, methods, interface
dispatch, sys-ops, `spawn`/`await`, `throw`, `??`, indirect calls, callees
without source, a `panic` with a computed message, `int` literals outside the
63-bit range, and recursion anywhere in the admitted call graph.

`Rejection::Invalid(reason)` means the MIR violated an invariant a checked
program must hold (a compiler bug): lowering errors, a read before definite
assignment, type mismatches between an assignment and its local or between a
call and its callee, an irreducible CFG, unknown block ids, or generated
tokens that do not parse.

One MIR quirk is handled deliberately: the fall-through edge of
`while (true) { .. }` assigns `null` to the `int` return place. The checker has
proven that edge dead, so the assignment is emitted as
`return Err(Panic::Unreachable)` rather than rejected.
