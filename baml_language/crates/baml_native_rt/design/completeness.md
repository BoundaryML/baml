# Completeness: what the native backend cannot do yet

A checklist of every capability `baml-cli run` (the bytecode VM) has and the native backend lacks. Measured at `aa1e227301` on `claude/baml-native-backend`, arm64 macOS. Nothing here is about speed.

Re-marked at `881e4c1dfc`, after values became two words. What was done again: the census of the standard library, the conformance directory and `baml_tests/baml_src` ("Second census" at the end), and each snippet whose item below says "now". What was not: the other corpora, the natives table, sections 4 to 6, and every `file:line` reference and bracketed count, which are as they were at `aa1e227301` unless the item says otherwise. An item that says "now" was changed by this re-marking; the snippet under it was run through `native_pack --coverage` again.

Legend: `- [ ]` missing, `- [~]` partial, `- [x]` suspected missing, in fact done. "refused" means `native_pack --coverage` printed the blocker shown. Every snippet was run through `native_pack --coverage <file> main` unless it says "not confirmed". Counts in brackets are from the census at the end: `[stdlib S, corpus C / E]` = S standard library functions and C functions of the test corpora blocked by this kind, in E corpus entry points.

Paths: `cg/` is `crates/baml_compiler2_native/src/`, `rt/` is `crates/baml_native_rt/src/`, `std/` is `crates/baml_builtins2/baml_std/`.

Headline numbers:

- Standard library, BAML source: 1,150 functions with a body. Now 691 compile with everything they reach when checked as an entry (551 before) and 830 are themselves inside the subset (678). No function that compiled before is refused now.
- Standard library, Rust builtins: 650. 387 can be called from native code (310 runtime symbols, 71 compiled inline, 6 trace option makers accepted and ignored). 263 cannot. Not counted again.
- Test corpora, at `aa1e227301`: 12,826 functions checked as entries, 6,946 compile with everything they reach. Now, for the two corpora checked again: `baml_tests/baml_src` 4,073 of 8,611 (3,678 before; 405 newly compile, 10 that compiled are refused, all for a type test); the conformance directory 1,795 of 1,795 (it was 1,559 of 1,559 at `279f25e334`, and has more cases now). The speedtest `main`s were 77 of 78 and were not checked again.
- 94 distinct blocker kinds are hit in practice. 27 more exist in the code and were never hit.
- One blocked function anywhere in what the entry reaches fails the whole build. There is no fallback to the interpreter (`cg/lib.rs:179-181`).

## 1. Lowering gaps

`cg/check.rs` refuses a function with `Blocker::new(construct, detail)`. `cg/types.rs`, `cg/program.rs`, `cg/json.rs`, `cg/natives.rs` and `cg/dispatch.rs` add more. Ordered by how much corpus code each blocks.

### Types with no native representation

`native_ty` (`cg/types.rs:178-336`) returns `None` and the checker reports `<kind> value`, `parameter is a <kind> value`, `result is a <kind> value` or `class field that is a <kind> value` (`cg/types.rs:393`, `429`, `cg/check.rs:214`).

- [ ] **`type` values (`reflect.Type`).** `other value`, `class field that is a other value` (`output_type`, `ty`, `returns`), `type value` (`cg/check.rs:1121`). No arm in `native_ty`; `type_kind` calls it "other" (`cg/types.rs:389`). [stdlib 103 + 41 + 20 + 5, corpus 1,866 + 895 + 161 + 72 / 2,573]. The largest single kind. Every LLM function carries an `output_type: type`.
  ```baml
  function main() -> string { let t = int.type(); `${t is reflect.Type}` }
  // blocked user.main  other value  _1
  ```
- [ ] **Media (`image`, `audio`, `pdf`, `video`).** `media value`, also as parameter, result and class field. [stdlib 16 + 4 + 6, corpus 437 + 208 + 76 + 10 / 1,744]. None of the 32 `baml.media` builtins is ported either.
  ```baml
  function main() -> string {
    let img = baml.media.Image.from_url("https://example.invalid/a.png", null);
    img.url() ?? "none"
  }
  // blocked user.main  standard library native that is not ported  baml.media.Image.from_url
  ```
- [x] **Optional scalars inside containers.** Now done: an `int?`, `float?` or `bool?` is one value like any other, so it is an array element, a map value, a future's value and a type argument. Both snippets compile.
  ```baml
  function main() -> int { let xs: int?[] = [1, null, 3]; xs.length() }
  // ok
  function main() -> int { let f = spawn { if 1 > 0 { 3 } else { null } }; (await f) ?? 0 }
  // ok
  ```
- [~] **Maps.** Keys are `string`, `int`, `bool`, an enum or `float` only; in code compiled once for a type parameter the key may be that parameter. Now a value may be an optional scalar (`map<string, int?>` compiles). `map<K, int>` for a class `K` with its own `hash` is still refused.
  ```baml
  class K { a: int }
  function main() -> int { let m: map<K, int> = {}; m.length() }
  // blocked user.main  map value  _2
  ```
- [~] **Unions.** A union of supported members is a tag and a payload, with no box, and works, with `match`. `union value` appears when a member has no native type (media, `type`, an optional-scalar list). [stdlib 91, corpus 447 / 2,208]
- [ ] **`never` as a declared type.** `result is a never value`, `never value`. A function declared `-> never` (`baml.sys.panic`, `baml.sys.exit`, `baml.http.Server.serve`) is refused when it is not inlined away. Called directly from `main` both inline and compile. [stdlib 7, corpus 52 + 37 / 331]
- [~] **Generics.** Now a function with type parameters is compiled once: inside it a value of a type parameter is a tag and a payload, and it takes a type object only for a parameter it tests or hands to `baml.json`. `type variable value`, `result is a type variable value` and `parameter is a type variable value` are gone from the census (they were 113 + 40 + 10 standard library and 778 + 190 + 153 `baml_src` functions). Both snippets that were refused compile:
  ```baml
  function biggest<T extends baml.ops.Compare>(xs: T[]) -> T {
    let best = xs[0]; for (let x in xs) { if x > best { best = x; } } best
  }
  function main() -> int { biggest([3, 9, 2]) }
  // ok
  class P { x: int }
  function main() -> int { let ps = [P { x: 3 }]; ps.sort_by_key((p: P) -> int { p.x }); ps[0].x }
  // ok
  ```
  What is left, all because no object says its type arguments (an instance's type parameter slots, an array's and a map's type words are not written; `ideas/uniform-values-convention.md`, status):
  - `call with type arguments (a type no tag and class object decide)`: a call that names, for a parameter the callee tests, a type such as an array type or a generic class instantiation. [now: stdlib 6, `baml_src` 44]
  - A type test between two instantiations of one class or against an array type (under "Type tests").
  - `xs.sort()` and `join` on a `T[]` inside generic code that is not inlined: `interface method call (sort: of elements with a compare of their own)`. Regressed: before, the copy made for `int` sorted.
    ```baml
    function sorted<T extends baml.ops.Compare>(xs: T[]) -> T[] { if xs.length() > 100 { return sorted(xs); } xs.sort(); xs }
    // blocked user.sorted  interface method call  sort: of elements with a compare of their own
    ```
  - Two different implementations of one method for one class at different type arguments: `implementations for one class at different type arguments`.
  - `function value that tests a type argument`, `closure that tests a type argument`: a lambda cannot be passed a type object. [now: `baml_src` 20 + 11]

### Calls

- [x] **Defaulted arguments.** Now done for every type: an argument left to its default is passed with the tag `OMITTED` and the callee computes the default (`calls.md`, `ideas/default-arguments.md`). `defaulted argument` is gone from the census (43 standard library and 1,329 `baml_src` functions). Both snippets compile. Left: a native or an intrinsic cannot take an omitted operand (`log`'s optional one), and a class's own `to_json`, `from_json` or `eq` with extra defaulted parameters is refused.
  ```baml
  function f(n: int, limit: int? = null) -> int { if n <= 0 { return 0; } 1 + f(n - 1) }
  function main() -> int { f(3) }
  // ok
  function base(n: int) -> int { if n > 50 { base(n - 1) } else { n } }
  function f(n: int, by: int = base(3)) -> int { if n <= 0 { return 0; } by + f(n - 1, by = by) }
  function main() -> int { f(3) }
  // ok
  ```
- [ ] **Globals (`const`).** `global read` (`cg/types.rs:1101`), `call of a non-function constant` (`cg/check.rs:496`). There is no initializer pass and no storage. [corpus 532 + 7 / 727]
  ```baml
  const LIMIT: int = 5;
  function main() -> int { LIMIT + 1 }
  // blocked user.main  global read
  ```
- [~] **Function values.** Closures over cells, plain functions and bound methods work. Now also: a function with type parameters as a value, a bound method of a generic class, and a function with defaulted parameters where a type with fewer is expected (through a remap thunk). Every call through a value checks the call layout. Refused at `aa1e227301` (not all checked again): a generic function as a value, a function without source, virtual and generic function makers, a bound method with type arguments or on a non-reference receiver. `function value` (`cg/types.rs:1098-1100`, `cg/check.rs:1117-1119`), `bound method value` (`cg/check.rs:1363-1374`), `call of a value that is not a function` (`1694`), `closure that tests a type argument` (`1339`). [stdlib 21 + 19 + 12, corpus 631 + 50 + 12 + 59 + 33 / 2,311]
- [~] **Interface method dispatch.** Now works for any implementing type, an `int`, a `string` or an array among them (a row per tag beside the rows in class objects), for an implementation with more parameters than the interface (a method-table thunk), and for the operators of `int`, `float`, `bool`, `bigint` and `string` reached through a bound. The snippet below, refused before for `a type kept in a box`, compiles. `interface method call`, by detail, `baml_src` functions blocked now (295; 336 before):
  - 101 `random_int: implemented in Rust for a type only known when it runs`: a builtin behind dynamic dispatch whose stub a row cannot hold (it is not `async`, or has type parameters, or takes a described type).
  - 47 `to_json: a default body that tests its type argument`.
  - 29 `iter`, 16 `to_string`, 13 `lt`: `on a type with no implementation found`.
  - 15 `sort: of elements with a compare of their own`, 13 `hash: the language's own implementation`.

  [stdlib 13, corpus 474 / 517]
  ```baml
  interface Sized { function size(self) -> int throws never; }
  implements Sized for int { function size(self) -> int throws never { self } }
  class C { n: int
    implements Sized { function size(self) -> int throws never { self.n } } }
  function pick(i: int) -> Sized { if i % 2 == 0 { return C { n: i }; }; return 7; }
  function total(s: Sized, d: int) -> int { if d <= 0 { return s.size(); } total(s, d - 1) }
  function main() -> int { total(pick(1), 3) + total(pick(2), 3) }
  // ok
  ```
- [ ] **Interface fields.** `interface field access` (`cg/check.rs:1120`, store at `1435`). No model at all. It compiles only when the optimizer devirtualizes the read (`speedtest/interfaces/field-access-100k` `main` passes that way). [corpus 138 / 148]
  ```baml
  interface Named { name: string }
  class Dog { name: string
    implements Named {} }
  class Robot { serial: int, model: string
    implements Named { name as model } }
  function pick(i: int) -> Named { if i % 2 == 0 { return Dog { name: "rex" }; }; return Robot { serial: i, model: "r2" }; }
  function len(n: Named, d: int) -> int { if d <= 0 { return n.name.length(); } len(n, d - 1) }
  function main() -> int { len(pick(1), 3) + len(pick(2), 3) }
  // blocked user.len  interface field access
  ```
- [ ] **VM builtins outside the native table.** `runtime builtin (vm)` (`cg/check.rs:199`). The table is built from the `baml` package and the I/O builtins of `ai` and `reflect` only (`baml_builtins2_codegen/src/extract.rs:59-98`), so the 124 `$rust_function` builtins of `reflect`, `trace`, `ai` and `testing` are plain "no body". [stdlib 64, corpus 359 / 3,020]
- [ ] **Natives not ported.** `standard library native that is not ported` (`cg/natives.rs:221`), `standard library system operation that is not ported` (`cg/check.rs:1927`). See section 2. [stdlib 86 + 33, corpus 1,269
  + 243 / 3,329]
- [~] **`call that does not match its callee's layout`** (`cg/check.rs:741`, `786`, `1702`, `1861`, `cg/program.rs:532`), `argument of an unexpected type` (`1138`), `assignment between different types` (`1407`). Follow-on refusals: the callee or a value has a type the checker could not place. Real cause in the corpus: `baml.future.all` (below), `baml.String.is_empty` on a class-typed receiver, `function to function`, `null to class`. [stdlib 40 + 7, corpus 212 + 7 + 134 / 2,391]

### Tests and operators

- [~] **Type tests.** `type test`. Works on a union, an optional or `unknown` against a class, enum, scalar, `null`, an interface, a type parameter, and a union or literal a description decides. Refused, `baml_src` functions now (394; 438 before): 271 `of a type no tag and class object decide` (a `type` value, a map or array type inside the tested type), 53 `between array types`, 27 `of a literal, on a value of another type or of several`, 22 `of a type argument or an interface, on a value that is not a union`, 21 `between instantiations of one class`. The last is a regression: `x is Box<int>` on a union of two instantiations with scalar arguments compiled when each instantiation had its own type id, and ten `baml_src` functions that compiled are refused now, seven for this.
  ```baml
  function pick(n: int) -> int[] | string[] { if n == 0 { [1] } else { ["a"] } }
  function main() -> int { let v = pick(0); if v is int[] { 1 } else { 2 } }
  // blocked user.main  array construction where no array is expected  union
  // blocked user.main  type test  between array types
  class Box<T> { v: T }
  function pick(n: int) -> Box<int> | Box<string> { if n == 0 { Box<int> { v: 1 } } else { Box<string> { v: "a" } } }
  function main() -> int { let b = pick(0); if b is Box<int> { 1 } else { 2 } }
  // blocked user.main  type test  between instantiations of one class
  ```
- [ ] **`??` on `unknown`.** `null coalescing of a non-optional value` (`cg/check.rs:1977`). Works on optionals and on unions with `null`. [stdlib 3, corpus 426 / 2,068]
  ```baml
  function get(m: map<string, unknown>, k: string, d: int) -> unknown {
    if d > 0 { return get(m, k, d - 1); } m.get(k) ?? "none"
  }
  // blocked user.get  null coalescing of a non-optional value
  ```
- [ ] **Truthiness.** `unary operator on this operand type (truthy array)`, also `!array`, `truthy optional`, `truthy string`, `truthy map`, `truthy bigint`, `truthy uint8array` (`cg/types.rs:1340`). Only `bool` is tested. [corpus 93 / 152]
  ```baml
  function f(xs: int[], n: int) -> int { if n <= 0 { return 0; } if xs { 1 + f(xs, n - 1) } else { 0 } }
  // blocked user.f  unary operator on this operand type  truthy array
  ```
- [~] **Mixed binary operators.** `binary operator on these operand types` (`cg/types.rs:1239`): `class == null` on a value flow has not narrowed, `int == null`, `union == enum`, `class * float` (a user `Multiply`), `int + string`. `find(0) == null` on a `P?` compiles. [stdlib 4, corpus 84 / 563]
- [ ] **Arithmetic on unions of numbers.** `baml.ops.__union_add`, `_sub`, `_mul`, `_div`, `_rem`, `_neg` are not ported.
  ```baml
  function pick(n: int) -> int | float { if n > 0 { 1 } else { 2.5 } }
  function main() -> float { let v = pick(1) + pick(0); 1.0 }
  // blocked user.main  standard library native that is not ported  baml.ops.__union_add
  ```
- [ ] **`switch on a class tag`, `switch on a non-int value`, `type tag read`** (`cg/check.rs:1639`, `1659`, `1209`, `2040-2061`). A `match` the MIR lowers to a jump on the header tag of a non-union value, or on a string. [stdlib 3 + 2, corpus 6 + 6 + 4 / 529]
- [~] **Pattern matching on shapes.** `length of a non-array value` (`cg/check.rs:1216`), `index access on a non-array value`, `tuple index access on a non-tuple value`, `field access on a non-class value` (`cg/types.rs:983-1037`): list, tuple and class patterns against a union or `unknown` scrutinee. [corpus 45 + 7 + 22 + 19 / 72]. `tuple index that is not a constant` once.

### Text

- [~] **`to_string` and interpolation.** `Intrinsic::ToString` takes `int`, `float`, `bool`, `null`, an enum, `string`, `bigint` (`cg/check.rs:165-176`). An array, a class without its own `to_string`, a union: `builtin call on these argument types (ToString)` (`cg/check.rs:1731`, `1831`). A map: `interface method call (to_string: on a type with no implementation found)`. [stdlib 8, corpus 110 / 2,087]
  ```baml
  class P { x: int }
  function main() -> string { let p = P { x: 1 }; `p is ${p}` }
  // blocked user.main  builtin call on these argument types  ToString
  ```
- [ ] **`Array.join` of elements with their own `to_string`** (`cg/check.rs:561`). Only `string`, `int`, `float`, `bool` elements.
  ```baml
  class P { x: int
    implements baml.ToString { function to_string(self) -> string { "P" } } }
  function main() -> string { [P { x: 1 }, P { x: 2 }].join(",") }
  // blocked user.main  baml.Array.join of elements with their own to_string, which the runtime cannot call
  ```

### Statements the checker has no case for

- [ ] **`compiler intrinsic`** (`cg/check.rs:1433`). Corpus functions: `BindType(0)` 174 and `BindType(1)` 13 (a type argument bound to a local, as `baml.json.to<T>` in generic code and reflection do), `ApplyTraceHook` 30, `Log(Info)` 22, `Log(Warn)` 20, `Log(Debug)` 1. [stdlib 10, corpus 250 / 2,083]
  ```baml
  function main() -> int { log.info("hello"); 1 }
  // blocked user.main  compiler intrinsic  Log(Info)
  ```
- [ ] **Finalizers (`cleanup`).** `construction of a class with a finalizer` (`cg/check.rs:1272`), and `baml._cleanup_begin` is not ported. `defer` itself compiles. [stdlib 1, corpus 31 / 371]
  ```baml
  class R { log: string[]
    function cleanup(self) -> void { self.log.push("cleaned"); } }
  function main() -> int { let r = R { log: [] }; r.cleanup(); r.log.length() }
  // blocked user.main  construction of a class with a finalizer
  // blocked user.main  standard library native that is not ported  baml._cleanup_begin
  ```
- [ ] **`await` of the first of several** (`Terminator::AwaitAny`, `cg/check.rs:1960`). Blocks `baml.future.race`, `any`, `with_timeout`. [stdlib 3, corpus 22 / 881]
- [~] **`spawn`, `await`.** `spawn (of a plan with another layout)` (`cg/check.rs:1936`), `await (of a value that is not a future)` (`1951`). Follow-ons of a future type the checker could not place: a future of an optional scalar, a generic `Plan<T, E>`. [stdlib 6, corpus 35 + 59 / 1,927]
- [ ] **Trace hooks and trace options as values.** `trace hook settings` (`cg/check.rs:1091`), `trace options (held as a value)` (`1719`). [corpus 30
  + 41 / 66]
- [ ] **`package value`** (`Rvalue::CurrentPackage`), **`interface bound test`** (`Rvalue::SatisfiesBound`) (`cg/check.rs:1112`, `1122`). [corpus 18
  + 18]
- [~] **JSON.** `baml.json` works for scalars, strings, arrays, string-keyed maps, classes, enums, unions, `json`. Refused in `cg/json.rs:383-447`, `494-529`, `544`, `598-625`: `json of a bigint` [26], `of a tuple` [15], `of a class with a @json.flatten or @json.fallback field` [15], `of a type with its own to_json or from_json` (an enum) [10], `of a generic class` [8], `of a uint8array` [4], `of a function` [2]; in code and not hit: `of a value of an interface type`, `recursive alias`, `type parameter`, `media value`, `future`.
  ```baml
  class P { x: bigint }
  function main() -> string { baml.json.stringify(P { x: 5n }) }
  // blocked user.main  json of a bigint  Bigint
  function main() -> string { baml.json.stringify((1, "a")) }
  // blocked user.main  json of a tuple
  ```
- [ ] **`==` when a generic class or an enum has its own `eq`.** `comparing values in a program with a type whose own eq the runtime cannot call` (`cg/program.rs:346`). [corpus 4]. Tested by `comparing_values_with_an_eq_the_runtime_cannot_call_is_refused`.

### In the code, never hit by the census

`MIR lowering failure`, `unterminated block`, `call of a function without source`, `call of a generic function`, `call result stored outside a local`, `operator method call (with other arguments)`, `standard library native with an unsupported signature`, `discriminant of a non-enum value`, `map construction where no map is expected`, `map index that is not of the key type`, `index access on a non-map value`, `tuple construction`, `class instance construction that does not match the layout`, `field outside its class layout`, `null coalescing into another type`, `array loop local used outside its loop`, `fresh cell for a local that has none`, the six closure and capture checks (`cg/check.rs:1343-1354`, `cg/types.rs:900`, `1043`, `cg/program.rs:83`, `570`), `eq with a layout the runtime cannot call`, `to_json or from_json with a layout the runtime cannot call`, `json of an enum without source`, `json of a type built from a type parameter`. Most guard MIR shapes the lowering does not produce.

### Refusals outside the checker

- [ ] `CompileError::EntryTakesArguments`, `EntryResultNotPrintable` (`cg/lib.rs:163-165`). See section 3, program arguments.
- [ ] A spawn plan whose semaphore or cancel-token list is not empty when it runs calls `baml_rt_spawn_unsupported` and ends the program (`cg/codegen/spawn.rs:154-180`, `rt/sched.rs:1080`).

## 2. Natives not ported

650 builtins are implemented in Rust for the interpreter (536 `$rust_function`, 114 `$rust_io_function`). The native table (`native_table.rs`, generated by `baml_compiler2_native/build.rs:81-88`) holds 526 of them.

| | count |
|---|---|
| runtime symbol, listed in `rt/../natives/*.txt` | 310 |
| compiled inline, no symbol: 64 operator impls (`cg/check.rs:587-646`), `Array.length`, `Array.push`, `String.char_count`, `_to_string_default`, `_to_string_primitive`, `Map.index`, `sys._sleep` | 71 |
| in the table, not callable | 145 |
| not in the table (`reflect` 92, `trace` 24, `ai` 6, `testing` 2), of which 6 trace option makers are accepted and ignored (`cg/check.rs:86-91`) | 124 |

### In the table

| module | callable / declared | missing (`*` = the signature has a type natives cannot take) |
|---|---|---|
| `ai` | 0 / 10 | `OutputFormat._render*`, `internal._aws_resolve_region`, `internal._aws_sign_request*`, `internal._gcp_access_token`, `internal._gcp_project_id`, `internal._gcp_quota_project_id`, `internal.build_output_format*`, `internal.get_return_type*`, `internal.reaches_media*`, `internal.render_output_format*` |
| `baml` (top level) | 11 / 15 | `_cleanup_begin`, `_compare_structural_default*`, `_hash_structural_default*`, `deep_copy` |
| `baml.Array` | 18 / 29 | `every*`, `filter*`, `find*`, `find_index*`, `find_last*`, `find_last_index*`, `flat_map*`, `map*`, `reduce*`, `some*`, `sort_by*` (all have stand-ins, below) |
| `baml.Bigint` | 8 / 8 | |
| `baml.Float` | 42 / 42 | |
| `baml.Int` | 12 / 12 | |
| `baml.Map` | 10 / 10 | |
| `baml.String` | 43 / 43 | |
| `baml.Uint8Array` | 16 / 16 | |
| `baml.archive` | 0 / 3 | `entries`, `extract`, `read_file` |
| `baml.crypto` | 15 / 15 | |
| `baml.csv` | 0 / 31 | `Reader._feed`, `Reader._feed_eof`, `Reader._mark_closed`, `Reader._mark_exhausted`, `Reader._poll`, `Reader._poll_headers`, `Reader.position`, `Reader.skipped`, `Reader.skipped_count`, `Record.fields`, `Record.length`, `Record.position`, `Record.to_map`, `Writer._bytes_written`, `Writer._encode_header`, `Writer._encode_record*`, `Writer._mark_closed`, `Writer.records_written`, `Writer.text`, `_buffer`, `_cell_by_index`, `_cell_by_name`, `_decode_record`, `_encode_row`, `_encode_rows`, `_reader`, `_to_markdown`, `_to_markdown_records`, `_try_decode`, `_validate_columns`, `_writer` |
| `baml.env` | 1 / 1 | |
| `baml.errors` | 0 / 3 | `Context._to_string_impl`, `StackTrace._to_string_impl`, `UnknownError._preserve_context*` |
| `baml.fs` | 25 / 25 | |
| `baml.future` | 0 / 9 | `Future._id*`, `Future.cancel*`, `Future.is_cancelled*`, `Future.is_error*`, `Future.is_result*`, `Future.is_settled*`, `Future.state*`, `_task_report`, `_task_stack` |
| `baml.glob` | 2 / 3 | `Glob.scan` |
| `baml.hash` | 3 / 3 | |
| `baml.host` | 0 / 1 | `call_host_value*` |
| `baml.http` | 11 / 14 | `Server._serve*`, `Server.bind`, `TlsConfig._new` |
| `baml.io` | 5 / 5 | |
| `baml.json` | 7 / 8 | `_schema*` |
| `baml.media` | 0 / 32 | for each of `Audio`, `Image`, `Pdf`, `Video`: `base64`, `from_base64*`, `from_file_content*`, `from_url*`, `infer_mime_type`, `mime_type`, `name`, `url` |
| `baml.net` | 11 / 11 | |
| `baml.ops` | 65 / 71 | `__union_add*`, `__union_div*`, `__union_mul*`, `__union_neg*`, `__union_rem*`, `__union_sub*` |
| `baml.path` | 4 / 4 | |
| `baml.random` | 8 / 8 | |
| `baml.regex` | 10 / 10 | |
| `baml.sap` | 0 / 5 | `_ParseCache._parse_final*`, `_ParseCache._parse_partial*`, `_new_parse_cache*`, `_reads_shapes`, `coerce*` |
| `baml.spawn` | 0 / 4 | `CancelToken.any`, `CancelToken.cancel`, `CancelToken.is_cancelled`, `CancelToken.new` |
| `baml.sync` | 10 / 14 | `_enter`, `_exit`, `_held`, `_wait` (all of `baml.sync.atomic`) |
| `baml.sys` | 12 / 21 | `_subprocess`, `Subprocess.close`, `Subprocess.kill`, `Subprocess.wait`, `ReadPipe.close`, `WritePipe.close`, `Read$for$ReadPipe.read`, `Write$for$WritePipe.flush`, `Write$for$WritePipe.write_some` |
| `baml.time` | 31 / 31 | |
| `baml.toml` | 0 / 2 | `Table._to_string_impl`, `Table.parse` |
| `baml.ws` | 0 / 4 | `_connect`, `WebSocket.close`, `WebSocket.next`, `WebSocket.send` |
| `baml.yaml` | 1 / 1 | |
| `reflect` (I/O) | 0 / 2 | `Package._compile*`, `Session._compile*` |

The six that block the most corpus functions: `Write$for$WritePipe.write_some` 494, `media.Image.from_base64` 105, `ai.internal.build_output_format` 99, `future.Future.cancel` 79, `spawn.CancelToken.new` 49, `media.Image.from_url` 40.

Confirmed refused: `baml.csv.to_markdown`, `baml.toml.Table.parse`, `baml.sys.subprocess`, `baml.ws.connect`, `baml.http.Server.bind`, `baml.deep_copy`, `baml.sync.atomic`, `baml.sap.parse<P>`, `Future.is_settled`, `Future.cancel`, `CancelToken.new`.

### Not in the table

Reported as `runtime builtin (vm)` on the builtin itself.

- [ ] `reflect`, 92: `Type` 19 (`kind`, `meta`, `of_value`, `optional`, `array`, `to_baml`, `_implements`, `_to_string_impl`, `_validate_renderable`, `as_array`, `as_class`, `as_enum`, `as_function`, `as_interface`, `as_literal`, `as_map`, `as_primitive`, `as_tuple`, `as_union`), `Package` 19 (`classes`, `enums`, `functions`, `globals`, `interfaces`, `tests`, `diagnostics`, `find_*` and `get_*` for class, enum, function, global, interface, `_finish`, `_with_types`), `Class` 7 (`_new`, `_extends`, `builder`, `fields`, `get_field`, `instantiate`, `metadata`), `class.Builder` 4, `class.PendingType` 5, `Enum` 6, `Tuple` 4, `Map` 3, `Session` 3, `Union` 2, `Literal` 2, `Function` 2, `Array.element_type`, `Interface.implementation`, `interface.Implementation.field`, `call_any`, `signature`, `_render_cause`, and the 10 `TypeView` impl methods of the kind classes.
- [ ] `trace`, 18 of 24: `Span._activate`, `Span.close`, `Span.fail`, `Span.fail_with_context`, `Span.finish`, `Span.id`, `Span.is_closed`, `ReservedSpan.id`, `ReservedSpan.inspect`, `Options.context`, `Options.inspect`, `Options.reserve`, `start_span`, `_create_span`, `_passthrough`, `context`, `current_context`, `current_span_id`.
- [ ] `ai`, 6: `Prompt.messages`, `Prompt.text`, `internal.assemble_prompt`, `internal._sanitize_text`, `internal._sanitize_request_header`, `internal._describe_unreadable_credential_file`.
- [ ] `testing`, 2: `_test_name_count`, `_testset_name_count`.

### Ported, with restrictions

- [~] **Generic natives.** Now a type argument may be any type with a native representation, an optional scalar among them; seven natives are passed type objects and the rest read tags (`natives.md`). Still refused, `baml_src` functions now (40): `Array.slice: called with other type arguments` 28, `json._to`, `json._decode`, `Array._copy`, `Map.set: at a type with no native representation` 11, `Map.index: on unknown` 1.
- [~] **`Array.join`**: section 1.
- [~] **Natives cannot take closures.** None of the 11 closure-taking `Array` methods has a signature. The inliner swaps in a BAML sibling (`baml_compiler2_mir/src/optimize/inline.rs:517-551`, `std/baml/containers.baml`): `_inlined_filter`, `_inlined_map`, `_inlined_some`, `_inlined_every`, `_inlined_find`, `_inlined_find_index`, `_inlined_reduce`, and `_driver_sort_by`, `_driver_find_last`, `_driver_find_last_index`, `_driver_flat_map`. `map`, `filter`, `reduce`, `sort_by` with a closure literal and `map` with a function parameter are confirmed to compile. A `map` whose result element is an optional scalar is refused (`list of unsupported elements value`).
- [~] **Defaulted arguments of a native.** Now: a constant default is passed by the caller as before; an omitted operand of any type is refused, since no native computes a default.
- [~] **A native behind interface dispatch** must now be `async`, have no type parameter and take no parameter that needs its static type described (`cg/program.rs`, `dispatched_native`); its stub takes the two-word convention, so `float`s and unions are no longer a limit. Otherwise `interface method call (implemented in Rust for a type only known when it runs)`.
- [~] **A native cannot call back into BAML except through `scope.call_baml`**, used by `baml.json` hooks and `eq`. `to_string`, `hash` and `cmp` of user types are never called from Rust.
- [~] **JSON text and `json` values past the static type.** An integer past 64 bits parses to a `bigint` inside a `json`, as on the interpreter, and is written back as its digits; decoded into `int` it is `DecodeError` "expected integer in the BAML int range [-2^63, 2^63 - 1]", into `float` the nearest one (cases `json_parse_past_int*`, `json_decode_past_int*`, `json_to_past_int_in_a_union`). Now an array or map of anything held as `unknown` serializes: every slot's tag says what it holds, and an enum variant is a pointer to its object, which names its enum (`json_unknown_enum_array` is no longer in `differences.txt`). Gap: decode depth is capped at 128 (`json_to_deep`, undecided).
- [~] **`SseStream.close`** ends the stream natively, does nothing on the interpreter while the program holds it (`http_sse_closed_early`).

## 3. Language features

- [ ] **LLM functions.** Absent. A function with `client:` and `prompt:` is lowered to a call into `ai.Agent`, which is refused throughout.
  ```baml
  class Resume { name: string }
  function Extract(text: string) -> Resume {
    client: "openai/gpt-4o-mini"
    prompt: `Extract a resume from ${text}`
  }
  function main() -> string { Extract("x").name }
  // blocked user.Extract<_>       defaulted argument  of an int or float
  // blocked user.Extract@spec     defaulted argument  of an int or float
  // blocked trace._passthrough    runtime builtin  vm
  // blocked ai.<(ai.Agent as ai.Runner)>.run<unknown>  null coalescing of a non-optional value
  // blocked ai.events.guard<unknown>  type test  of a type without one type id
  // ... 1,812 distinct ai.* and provider functions are blocked across the corpus
  ```
  - [ ] Prompt rendering and Jinja: `ai.internal.assemble_prompt`, `ai.Prompt.text`, `ai.Prompt.messages`, `ai.OutputFormat._render`, `ai.internal.render_output_format`, `build_output_format` not ported.
  - [ ] Schema-aligned parser: `baml.sap` 0 of 5.
  - [ ] Providers (`openai`, `anthropic`, `google`, `aws`, `vercel`, `claude_code`, `typesafeai`): BAML source over `baml.http`, blocked by defaulted arguments, media, `type` values. AWS signing and GCP tokens (`ai.internal._aws_*`, `_gcp_*`) not ported.
  - [ ] Streaming: `baml.http._send_sse` and `SseStream` are ported, but `ai.stream.TurnStream.next` is refused (`array construction where no array is expected`, `type test`) and partial parsing needs `_ParseCache._parse_partial`.
  - [ ] Retry, fallback, round robin, timeouts (`std/ai/timeouts.baml`): need `baml.future.with_timeout`, refused.
  - [x] A `client Name = ...` declaration alone does not block a program that never calls it (confirmed).
- [ ] **`test` blocks.** Absent. No entry point runs them (`native_pack` takes one function). All 507 `$init_test_*` functions of `baml_tests/baml_src` are refused through what they reach; `testing._test_name_count` is not in the table. `assert.equal` alone compiles.
- [ ] **Media types.** Absent. Section 1.
- [ ] **Type builder, dynamic types, reflection, `type` values.** Absent. `reflect` 0 of 94 builtins, `type` has no native representation, `BindType` is refused. `speedtest/interfaces/mint-classes-with-impls-200` is the one speedtest `main` that fails, for this.
- [~] **Generics.** Now compiled once. `class Box<T>`, `unbox<T>`, generic closures, bounds with operators confirmed. Gaps in section 1: no object says its type arguments.
- [ ] **Interfaces with fields.** Absent. Section 1.
- [x] **Enums.** Plain variants, `match`, `==`, interpolation, map keys work. Enums carry no data in the language (`std` and the corpus declare none; not confirmed against the grammar).
- [~] **Unions and optional scalars.** Built. Now optional scalars work in containers. Gaps: union arithmetic, `??` on `unknown` (still refused, checked again).
- [~] **Pattern matching and type tests.** Built for unions, literals, enums, tuples (`cg/narrow.rs`). Gaps: array types, generic instantiations, shape patterns on unions, truthiness.
- [~] **Tuples.** Built: construction, destructuring, as results (`cg/types.rs:74-77`, confirmed). Gaps: no JSON, a tuple with an unsupported element is `tuple value` [corpus 15 + 10 + 3].
- [~] **`bigint`.** Built: literals, arithmetic, `to_string`, 8 of 8 `Bigint` natives, inside a `json` value both ways. Gaps: JSON of a value whose static type is `bigint` is refused, no truthiness.
- [~] **Closures and bound methods.** Built (`calls.md`). Gaps in section 1.
- [~] **String formatting.** Scalars only. Section 1.
- [ ] **Finalizers.** Absent. Section 1.
- [ ] **`spawn` plans with semaphores and cancel tokens.** Absent. `spawn with s { ... }` is refused at check (`type variable value`), and a plan that gets through ends the program (`rt/sched.rs:1080`). `Semaphore.with` and `acquire` outside a plan work.
  ```baml
  function main() -> int {
    let s = baml.sync.Semaphore.new(1);
    let a = spawn with s { 1 };
    await a
  }
  // blocked user.main  type variable value  _11 has no one type
  ```
- [ ] **Cancellation and timeouts.** Absent. `Task.cancel` is a reserved word in the task record (`rt/sched.rs:159`), nothing sets the poll word (`control.md`), `CancelToken` 0 of 4, `Future.cancel` not ported, `AwaitAny` refused. `baml.sys.run(..., timeout = ...)` does work (`a_program_stopped_at_its_limit_is_killed_and_waited_for`).
  ```baml
  function main() -> int throws baml.errors.Timeout {
    baml.future.with_timeout(baml.time.Duration.from_milliseconds(50), () -> int { 1 })
  }
  // blocked baml.future.with_timeout<int, _>  standard library native that is not ported  baml.spawn.CancelToken.new
  // blocked baml.future.with_timeout<int, _>  await of the first of several
  ```
- [ ] **Future combinators and state.** `baml.future.all`, `race`, `any`, `all_settled` are refused; `is_settled`, `is_error`, `is_result`, `state`, `_id` are not ported.
  ```baml
  function main() -> int { let fs = [spawn { 1 }, spawn { 2 }]; let rs = await baml.future.all(fs); rs[0] + rs[1] }
  // blocked user.main  call that does not match its callee's layout  baml.future.all
  // blocked .<lambda(<lambda(all, 0)>, 1)><int, _>  standard library native that is not ported  baml.future.Future.is_error
  ```
- [ ] **`trace` and telemetry spans.** Absent. Trace attachments on calls are dropped (`cg/check.rs:1427-1431`: "native code records none"), 18 `trace` builtins have no body, trace hooks are refused, the task record's telemetry node is reserved (`rt/sched.rs:161`), popped frames record nothing (`control.md`, "Not built"). No event reaches any sink.
- [ ] **`log`.** Absent. `compiler intrinsic (Log(Info))`.
- [~] **Env and config.** `baml.env.get` ported (1 of 1, confirmed). `baml.toml` 0 of 2. `baml.toml` manifests, `[scripts]` and test profiles are CLI features with no native counterpart.
- [ ] **Globals.** Absent. Section 1.
- [~] **Packages, imports, multi-package projects.** `coverage` and `compile` take a whole database, and the census compiled multi-file, multi-namespace projects through them. `native_pack` loads one file (`examples/native_pack.rs:75-83`). Dependencies between source roots: not confirmed. `reflect.Package` values: absent.
- [~] **Program arguments.** The entry takes no parameters (`EntryTakesArguments`, confirmed). `baml pack` builds a CLI parser from the signature (`baml_cli/src/pack_command.rs:19-22`); native has none. `baml.sys.argv()` works and holds only the program's own arguments, where `baml-cli run` also gives the binary and the target (`differences.txt`, `sys_argv`).
- [~] **Entry result.** Must be `int`, `float`, `bool`, `string` or `null` (`EntryResultNotPrintable`, confirmed for a class and an array). `baml-cli run` prints any value.
- [x] **Exit codes.** `baml.sys.exit(n)` throws `baml.panics.Exit`, which `baml-cli run` turns into the process status (`bex_engine/src/lib.rs:818`, `baml_cli/src/run_command.rs:1247`), and so does the native runtime when nothing in the entry task catches it (`baml_rt_uncaught`, `rt/unwind.rs`): no message, the `defer`s run, the errors no `await` took are still reported, and a code past an `i32` is the nearest one (so the OS sees its low 8 bits). An `Exit` that leaves a spawned task uncaught ends the process with its code there and then, after that task's `defer`s, whether or not anything awaits the task (`baml_rt_task_fail`, `rt/sched.rs`); the interpreter reports it as an error no `await` took and goes on (`differences.txt`, `sys_exit_in_a_task_nothing_awaits`, interpreter-wrong). Cases `sys_exit*` in `tests/conformance/sys.baml`.
- [x] **stdin.** `baml.io.input` works (ran it; also `a_program_prints_and_reads_its_own_streams`).
- [~] **Signals.** SIGINT and SIGTERM end the process with status 130 and 143 (`stopped`, `rt/lib.rs`): the handler only calls `_exit`, and every write to stdout has flushed before it returns, so nothing printed is lost. A signal the program was started ignoring stays ignored. No `defer` and no finalizer runs. `baml-cli run` (ran it) ignores SIGINT and runs to the end, and on SIGTERM runs the `defer`s, prints `interrupted: N task(s) still running: ...` and exits 143. Test `a_program_asked_to_stop_exits_with_the_signal_status`.
- [ ] **Subprocess handles and pipes.** `baml.sys.run` and `shell` work. `baml.sys.subprocess`, `Subprocess`, `ReadPipe`, `WritePipe` are not ported.
- [ ] **HTTP server, WebSocket, TLS config, CSV, TOML, archives, `deep_copy`, `baml.sync.atomic`, structural `hash` and `cmp`, host callables.** Section 2.

## 4. Runtime gaps

- [ ] **Tracebacks have no file and line**, and no frame for an inlined function (`differences.txt`: `unwind_context_text`, `unwind_context_line`; ran: `Traceback ... in user.main`). `StackTrace._to_string_impl`, `Context._to_string_impl`, `_task_stack`, `_task_report` are not ported.
- [ ] **A task's stack does not grow.** 64 KiB per spawned task, 8 MiB for the entry, fixed (`rt/sched.rs`, `control.md` "Stacks"). Recursion in a task overflows where the interpreter goes on (`task_recursion_too_deep`, `task_overflow_caught`).
- [ ] **Task cancellation.** `tasks.md` "Not built". Section 3.
- [ ] **Preemption.** A task runs until it parks or ends (`tasks.md`). A loop that never awaits holds off timers and I/O completions (from the design note; not run).
- [ ] **More than one worker.** Everything in `threads.md`, "What assumes one worker" (items 1 to 34). `spawn` never runs in parallel.
- [x] **Deadlock on a semaphore.** A wait for a hold says that only another task can end it (`Scope::wait_for_a_task`), and the worker with nothing to run, no timer and no other I/O asks each such wait whether it is still held up (`Sched::sleep`, `rt/sched.rs`): if all are, `fatal: deadlock: every task waits for another`, status 70. The interpreter waits for good (ran it: killed after 15 s, `interrupted: 1 task still running: ... waiting for a semaphore slot`), so the case is in `differences.txt` (`sync_deadlock`) as one the interpreter hangs on, which the harness does not run on the interpreter; it still kills any run at 30 s. Assumes one worker: with several, a hold can be released while this one looks.
  ```baml
  function main() -> int { let s = baml.sync.Semaphore.new(1); let p = s.acquire(); let q = s.acquire(); 1 }
  ```
- [~] **The entry returns while tasks live.** A native program runs every task that can still run and ends once none can. A task at work (a 200 ms sleep, then a print) finishes and prints after the result, status 0, as on the interpreter (`task_main_returns_with_a_task_working`). One that waits for good (a second `acquire` of a held semaphore) is left at once, status 0, nothing said; the interpreter waits out its 15 s shutdown timeout, warns `abandoned 1 leaked background future ...` with the task's stack, status 0 (`differences.txt`, `sync_main_returns_with_a_task_waiting`, undecided).
- [ ] **A native cannot hold a heap address across an allocation** (`gc.md` "Not built": no pinning, no large-object space). A `uint8array` is copied to be held across a stop (`BytesRef::hold`). Capability limit for any future native that hands a buffer to the OS for a long read.
- [ ] **An object larger than the space forces a collection into a bigger space; memory is never returned to the OS** (`memory.md`, `gc.md`).
- [ ] **One I/O thread, one Tokio runtime** (`natives.md` "Not built").
- [ ] **Tail calls** (`calls.md` "Not built"): the collector's stack walk assumes none. Deep mutual tail recursion overflows; the interpreter's limit differs (not confirmed).
- [~] **Messages that differ from the interpreter.**
  - Uncaught throw: `error: uncaught throw: <value>`, exit 1, matching `baml run` (`rt/lib.rs:41-43`). The traceback above it differs.
  - Runtime aborts with no interpreter counterpart: `a throw found no handler` (`rt/unwind.rs:363`, exit 70), `a spawn with semaphores or cancel tokens is not supported by native code yet` (`rt/sched.rs:1080`), `deadlock: every task waits for another` (`rt/sched.rs:1113`), a panic inside a native (runtime built `panic=abort`, hook in `rt/native/drive.rs:217-235`).
  - A Rust native that overruns the guard page dies by signal with no message: not confirmed.
- [x] **Uncaught errors in tasks nobody awaited** fail the program after its result, as on the interpreter (`an_error_no_await_took_fails_the_program_after_its_result`).

## 5. Toolchain and platform

- [ ] **`baml pack` is not wired to this backend.** No crate depends on `baml_compiler2_native` (`rg baml_compiler2_native --glob Cargo.toml` finds only the workspace entry and the crate itself). `baml pack` appends bytecode to `baml-pack-host` (`baml_cli/src/pack_command.rs:19-22`). The only entry point is `examples/native_pack.rs`: one file, one function.
- [ ] **No fallback.** One refused function fails the build with the coverage report (`cg/lib.rs:179-181`). No mixed mode with the VM.
- [ ] **Cross compilation.** The ISA is `cranelift_native::builder()` (`cg/codegen.rs:433`), the runtime archive is built for the compiler's own target (`build.rs:136-139`), `cg/lib.rs:13` says "host-only".
- [~] **x86-64.** Stubs and the task switch exist for System V (`rt/stubs.rs`, `rt/sched/switch.rs`) and "have never been built or run" (`design/README.md`, "Not measured"). The map probes are assembly on arm64 only, with a Rust path elsewhere (`rt/map.rs`).
- [~] **Linux.** `link.rs:99-103` passes `-Wl,--gc-sections`; `rt/sched.rs` has a non-Apple branch. Never built or run here: not confirmed.
- [ ] **Windows.** `rt/stubs.rs:29` is a `compile_error!` off arm64 and System V x86-64; the link step assumes a `cc` driver.
- [~] **The link step** (`cg/link.rs:80-116`) needs on the host: `$CC` or `cc` as the linker driver, the system libraries `rustc --print native-static-libs` reported when the compiler was built, and the four runtime archives at the absolute paths baked in with `env!` at compile time. A compiler binary moved to another machine cannot link. There is no bundled linker and no bundled archive.
- [ ] **Debug info.** None. No DWARF from Cranelift (no `gimli` use in `cg/`), the runtime is built `debug=false` (`build.rs:23-33`). Function names come from the runtime's own tables; `lldb` sees symbols only.
- [~] **Cranelift is vendored and patched** (`vendor/cranelift-codegen`). Not a gap; upgrades need the patches carried.
- [ ] **Build of the compiler itself** runs a nested `cargo rustc` four times for the runtime (`build.rs:67-80`). A vendored or offline build of the workspace needs that to work too: not confirmed.

## 6. Test coverage gaps

Capabilities that compile and run with no conformance or differential case.

- [ ] **HTTPS.** Every HTTP case talks to `http://127.0.0.1` (`tests/conformance.rs:973-975`). `a_program_without_http_links_no_tls` checks only what is linked. `rustls`, certificate roots and `TlsConfig` (not ported) are never exercised.
- [ ] **`baml.sys.shell`.** `_shell` is listed `async` and compiles and ran here (`baml.sys.shell("true").exit_code` printed `0`). No case. `sys.run` has five.
- [x] **`baml.sys.exit(n)`.** Cases `sys_exit*`.
- [ ] **Death by signal** of the native program itself (SIGSEGV on the guard page). The harness records it (`conformance.rs:50-60`) and no case produces it. SIGINT and SIGTERM while parked are covered by `a_program_asked_to_stop_exits_with_the_signal_status`. `ProcessExit.signal` of a child is covered.
- [ ] **`spawn with` plans, `Future.cancel`, `with_timeout`.** Refused, and no case asserts the refusal text. Only two refusals are pinned: `comparing_values_with_an_eq_the_runtime_cannot_call_is_refused` and `a_refused_standard_library_call_names_the_function_and_says_why`.
- [ ] **Semaphore deadlock** (section 4).
- [ ] **`baml.sys.panic`** and `Unreachable`, `MapKeyNotFound`, `NegativeBitShift`, `AllocFailure` reports: not checked one by one (not confirmed which have cases).
- [x] **Float `sum` and `median`.** Covered: `tests/conformance/baml/reductions.baml` (`sum_float`, `sum_float_empty`, NaN, `median_odd`, even, empty).
- [x] **`baml.io.input`, `eprint`, `eprintln`**: `a_program_prints_and_reads_its_own_streams`.
- [x] **Every listed native but `_shell`** has its public name in some case (name search over `tests/`; the internal `sync._*`, `regex._*_template`, `Array._truncate`, `_median_float`, `ops.equals_equals` are reached through their wrappers).
- [x] **No test is disabled.** The four `#[ignore]`d runtime tests end their process and are run as children by a neighbouring test. The gaps are absent cases.

## Census

### How

`crates/baml_compiler2_native/examples/census.rs` (untracked) calls `baml_compiler2_native::coverage` with every function of a corpus as the entry and prints every blocker of every function it reaches. A function counts as blocked under a kind when any entry reaches it blocked that way. Checking a function as an entry overstates two things: a generic function has no type arguments (`type variable value`), and an interface call whose implementors the entry never constructs has no layout (`call that does not match its callee's layout`). Functions with a `$rust_function` body and required interface methods are skipped.

```sh
cd baml_language
CARGO_BUILD_JOBS=4 CARGO_PROFILE_FASTTEST_LTO=off \
  cargo build --profile fasttest -p baml_compiler2_native --example native_pack --example census
BAML_TELEMETRY=off target/fasttest/examples/census --stdlib > census_stdlib.tsv
BAML_TELEMETRY=off target/fasttest/examples/census <label> <dir-or-file> > <label>.tsv
BAML_TELEMETRY=off target/fasttest/examples/native_pack --coverage snippet.baml main
```

Raw output, the scripts (`run_corpora.sh`, `analyze.py`, `merge.py`, `detail.py`, `natives.py`, `natives_md.py`), a copy of `census.rs` and the snippets (`snip*/`) are in the session scratch directory `scratchpad/completeness/`. `kinds.tsv` there has every kind with counts and examples. Nothing was run on the interpreter and no corpus program was executed.

### Functions

| corpus | functions checked | inside the subset themselves | compile with all they reach |
|---|---|---|---|
| standard library (BAML source, all packages) | 1,150 | 678 | 551 |
| of which not generic | 1,021 | 674 | 547 |
| `baml_compiler2_native/tests/conformance` (`.baml.in` with dummy addresses) | 1,634 | 1,634 | 1,634 |
| `tools/speedtest/workloads/*/*/main.baml` (78 programs) | 330 | 321 | 320 |
| of which `main` | 78 | | 77 |
| `baml_tests/baml_src` | 8,611 | 5,552 | 3,678 |
| of which `$init_test_*` (test files) | 507 | 477 | 0 |
| `baml_tests/rust_diff` | 233 | 175 | 156 |
| `sdk_tests/fixtures/*` (5 of 6 load) | 616 | 414 | 382 |
| `sdk_tests/crates/csharp/*` | 180 | 118 | 113 |
| `tools/type_quiz` | 793 | 538 | 434 |
| `wrapper`, `wrapper_tests` | 394 | 298 | 208 |
| CLI fixtures, `bex_engine` resource workloads, `sdks/*` examples | 35 | 28 | 21 |
| all corpora but the standard library | 12,826 | 9,078 | 6,946 |

Did not type-check and were skipped: `demo2/baml_src`, `examples/stream_span_lifetime`, `sdk_tests/fixtures/host_reflect`, `test_simple.baml`. Not attempted: `baml_tests/projects/*` (broken on purpose), `baml_fmt/tests/cases`, `baml_tests/src/type_spec`.

Standard library skips: 536 `$rust_function`, 114 `$rust_io_function`, 6 compiler intrinsics, 1 `AwaitAny`, 1 `Spawn`, 42 required interface methods.

### Kinds

94 distinct kinds hit (standard library 54, corpora 92). Distinct blocked functions, standard library then corpora, and corpus entries that reach one:

| kind | stdlib | corpus | entries |
|---|---|---|---|
| other value (`type`) | 103 | 1,866 | 2,573 |
| defaulted argument | 43 | 1,531 | 2,154 |
| standard library native that is not ported | 86 | 1,269 | 2,878 |
| type variable value | 113 | 976 | 2,971 |
| class field that is a other value | 41 | 895 | 2,030 |
| type test | 64 | 668 | 2,360 |
| function value | 21 | 631 | 2,311 |
| global read | 0 | 532 | 727 |
| interface method call | 13 | 474 | 517 |
| union value | 91 | 447 | 2,208 |
| media value | 16 | 437 | 1,744 |
| null coalescing of a non-optional value | 3 | 426 | 2,068 |
| runtime builtin | 64 | 359 | 3,020 |
| list of unsupported elements value | 44 | 289 | 2,112 |
| compiler intrinsic | 10 | 250 | 2,083 |
| standard library system operation that is not ported | 33 | 243 | 2,554 |
| result is a type variable value | 40 | 234 | 1,574 |
| call that does not match its callee's layout | 40 | 212 | 2,391 |
| parameter is a media value | 4 | 208 | 747 |
| map value | 14 | 176 | 2,046 |
| parameter is a type variable value | 10 | 171 | 172 |
| parameter is a class value | 28 | 163 | 2,089 |
| parameter is a other value | 20 | 161 | 2,106 |
| class value | 32 | 156 | 2,131 |
| class field that is a union value | 33 | 145 | 1,967 |
| interface field access | 0 | 138 | 148 |
| assignment between different types | 0 | 134 | 417 |
| builtin call on these argument types | 8 | 110 | 2,087 |
| call with type arguments | 1 | 95 | 2,126 |
| unary operator on this operand type | 0 | 93 | 152 |
| parameter is a union value | 12 | 87 | 1,704 |
| binary operator on these operand types | 4 | 84 | 563 |
| result is a list of unsupported elements value | 17 | 82 | 1,527 |
| optional scalar or union value | 11 | 82 | 1,548 |
| result is a media value | 6 | 76 | 598 |
| result is a other value | 5 | 72 | 2,048 |
| array construction where no array is expected | 2 | 70 | 915 |
| class field that is a list of unsupported elements value | 11 | 67 | 1,880 |
| await | 6 | 59 | 1,927 |
| call of a value that is not a function | 0 | 59 | 40 |
| result is a never value | 7 | 52 | 331 |
| class field that is a function value | 19 | 50 | 314 |
| result is a union value | 25 | 46 | 1,946 |
| class field that is a type variable value | 5 | 45 | 197 |
| length of a non-array value | 0 | 45 | 34 |
| trace options | 0 | 41 | 66 |
| generic standard library native | 11 | 40 | 72 |
| never value | 0 | 37 | 308 |
| spawn | 0 | 35 | 293 |
| bound method value | 0 | 33 | 25 |
| parameter is a list of unsupported elements value | 13 | 33 | 67 |
| construction of a class with a finalizer | 1 | 31 | 371 |
| trace hook settings | 0 | 30 | 36 |
| json of a bigint | 0 | 26 | 15 |
| result is a optional scalar or union value | 10 | 24 | 31 |
| await of the first of several | 3 | 22 | 881 |
| tuple index access on a non-tuple value | 0 | 22 | 14 |
| field access on a non-class value | 0 | 19 | 15 |
| package value | 0 | 18 | 23 |
| interface bound test | 0 | 18 | 14 |
| class field that is a map value | 8 | 16 | 34 |
| tuple value | 0 | 15 | 16 |
| json of a tuple | 0 | 15 | 13 |
| json of a class with a @json.flatten or @json.fallback field | 0 | 15 | 4 |
| parameter is a function value | 12 | 12 | 17 |
| result is a map value | 1 | 12 | 1,961 |
| the other 28 kinds | 18 | 126 | |

Grouped by cause, corpora only, distinct blocked functions and entries that reach one (groups overlap):

| group | functions | entries |
|---|---|---|
| reflection and `type` values | 2,244 | 2,921 |
| LLM stack (any blocked `ai.*` or provider function, `@spec`) | 1,812 | 2,182 |
| test framework (`testing.*`, `$init_test_*` and their lambdas) | 1,638 | 532 |
| defaulted argument | 1,531 | 2,154 |
| native or system operation not ported | 1,464 | 3,329 |
| generic with no one type | 1,183 | 3,156 |
| function values | 680 | 2,348 |
| type test | 668 | 2,360 |
| containers of unsupported elements | 559 | 2,313 |
| union shapes | 555 | 2,291 |
| globals | 536 | 730 |
| media | 486 | 1,773 |
| interface method call | 474 | 517 |
| `??` on a non-optional | 426 | 2,068 |
| truthiness and mixed operators | 177 | 715 |
| await any, cancel, spawn plans | 147 | 2,077 |
| trace builtins, hooks, options | 145 | 2,022 |
| interface field access | 138 | 148 |
| `to_string` of non-scalars | 116 | 2,088 |
| `log` | 40 | 553 |
| finalizers | 38 | 377 |
| JSON shapes | 80 | 50 |

About 2,000 entries share one cause: they reach `ai.Agent.run`, and that closure alone contains most kinds.

### Natives

650 Rust builtins: 387 callable, 263 not. By package: `baml` 381 of 514, `trace` 6 of 24 (accepted and ignored), `ai` 0 of 16, `reflect` 0 of 94, `testing` 0 of 2. Per module: the table in section 2. Reproduce with `natives.py` over `target/fasttest/build/baml_compiler2_native-*/out/native_table.rs` (newest) and the `skip ... builtin Vm` lines of `census_stdlib.tsv`.

### Second census, at `881e4c1dfc`

The same `census.rs`, unchanged, built at `881e4c1dfc`, over three corpora; "before" is the same program built at `279f25e334`, the commit before two-word values. Compile-only: no corpus program was run. Outputs and the scripts (`analyze.py`, `worse.py`) are in the session scratch directory `scratchpad/uv-impl/census/`.

| corpus | functions checked | inside the subset themselves, before | now | compile with all they reach, before | now |
|---|---|---|---|---|---|
| standard library | 1,150 | 678 | 830 | 551 | 691 |
| `baml_compiler2_native/tests/conformance` (`.baml` files) | 1,559 then, 1,795 now | 1,559 | 1,795 | 1,559 | 1,795 |
| `baml_tests/baml_src` | 8,611 | 5,552 | 6,161 | 3,678 | 4,073 |
| of which `$init_test_*` | 507 | 477 | 460 | 0 | 0 |

Standard library: 140 functions newly compile and none that compiled is refused. `baml_src`: 405 newly compile and 10 that compiled are refused, every one for a type test (7 `between instantiations of one class`, 2 `of a literal, on a value of another type or of several`, 1 `of a type no tag and class object decide`). Distinct kinds hit: standard library 45 (54), `baml_src` 82 (91). Counting a function alone, 738 `baml_src` functions moved inside the subset and 129 moved out of it; of those 129, 119 did not compile whole before either, and 103 are now blocked in their own body by a native that is not ported where a function they call was before.

Distinct blocked functions per kind, for every kind whose count changed by 9 or more in `baml_src`, largest change first; then the standard library's counts. A function compiled once has one name where its copies had one each (`f<int, _>`, `f<unknown, _>`), so a falling count is partly fewer names.

| kind | `baml_src` before | now | stdlib before | now |
|---|---|---|---|---|
| defaulted argument | 1,329 | 0 | 43 | 0 |
| type variable value | 778 | 0 | 113 | 0 |
| media value | 363 | 169 | 16 | 12 |
| result is a type variable value | 190 | 0 | 40 | 0 |
| other value (`type`) | 1,564 | 1,377 | 103 | 84 |
| parameter is a type variable value | 153 | 0 | 10 | 0 |
| parameter is a class value | 105 | 0 | 28 | 0 |
| standard library native that is not ported | 1,002 | 1,102 | 86 | 77 |
| class value | 89 | 0 | 32 | 0 |
| union value | 227 | 165 | 91 | 57 |
| list of unsupported elements value | 189 | 134 | 44 | 23 |
| type test | 438 | 394 | 64 | 55 |
| function value | 584 | 540 | 21 | 7 |
| interface method call | 336 | 295 | 13 | 5 |
| call that does not match its callee's layout | 132 | 94 | 40 | 10 |
| global read | 190 | 154 | 0 | 0 |
| call with type arguments | 8 | 44 | 1 | 6 |
| result is a list of unsupported elements value | 57 | 23 | 17 | 3 |
| class field that is a type variable value | 34 | 3 | 5 | 0 |
| parameter is a list of unsupported elements value | 29 | 1 | 13 | 3 |
| standard library system operation that is not ported | 131 | 108 | 33 | 33 |
| runtime builtin | 155 | 132 | 64 | 63 |
| class field that is a function value | 47 | 24 | 19 | 0 |
| function value that tests a type argument | 0 | 20 | 0 | 1 |
| result is a other value | 53 | 34 | 5 | 4 |
| result is a union value | 23 | 6 | 25 | 2 |
| class field that is a union value | 41 | 24 | 33 | 29 |
| assignment between different types | 119 | 135 | 0 | 0 |
| optional scalar or union value | 43 | 28 | 11 | 5 |
| class field that is a other value | 733 | 719 | 41 | 38 |
| class field that is a list of unsupported elements value | 31 | 18 | 11 | 8 |
| parameter is a function value | 11 | 0 | 12 | 0 |
| await | 43 | 32 | 6 | 4 |
| closure that tests a type argument | 1 | 11 | 0 | 1 |
| spawn, result is a never value, never value, compiler intrinsic, null coalescing of a non-optional value | each 9 fewer | | | |

Kinds whose count rose, and why:

- `standard library native that is not ported`, 1,002 to 1,102. Not a native lost: 145 of the new names are blocked by `baml.sys.baml.io.Write$for$WritePipe.write_some`, which was not ported before either. They are functions the checker did not reach or name this way before: 42 were blocked by something else then (`type variable value` 35, `global read` 29), and 103 are the parts of LLM functions (`Extract@build_request` and the like) now checked under one name. No function that compiled is blocked by it.
- `call with type arguments`, 8 to 44, `function value that tests a type argument`, 0 to 20, `closure that tests a type argument`, 1 to 11: the refusals that replaced `type variable value` where a type argument has to reach code that tests it and cannot (a type no tag and class object decide; a lambda takes no type object). Real gaps, listed under "Generics".
- `assignment between different types`, 119 to 135 (57 new names, 41 gone; `class to class` 55): follow-on refusals in functions blocked by something else before, most in test lambdas. Not looked at one by one.
- `call of a value that is not a function` 59 to 67, `generic standard library native` 34 to 40 (`baml.json._to` and `_decode` at a media type), `json of a type with no native form` 0 to 4 (media): functions that were blocked earlier by another kind.
- In the standard library: `call with type arguments` 1 to 6 (`baml.errors.UnknownError.with_message`, `ai.tools._typed_arguments`, `baml.Array._inlined_map` with a class parameter), `await of the first of several` 3 to 4, and one function each for `function value that tests a type argument`, `closure that tests a type argument`, `field access on a non-class value`, `call of a value that is not a function`.
