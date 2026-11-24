# Completeness: what the native backend cannot do yet

A checklist of every capability `baml-cli run` (the bytecode VM) has and the native backend lacks. Measured at `aa1e227301` on `claude/baml-native-backend`, arm64 macOS. Nothing here is about speed.

Legend: `- [ ]` missing, `- [~]` partial, `- [x]` suspected missing, in fact done. "refused" means `native_pack --coverage` printed the blocker shown. Every snippet was run through `native_pack --coverage <file> main` unless it says "not confirmed". Counts in brackets are from the census at the end: `[stdlib S, corpus C / E]` = S standard library functions and C functions of the test corpora blocked by this kind, in E corpus entry points.

Paths: `cg/` is `crates/baml_compiler2_native/src/`, `rt/` is `crates/baml_native_rt/src/`, `std/` is `crates/baml_builtins2/baml_std/`.

Headline numbers:

- Standard library, BAML source: 1,150 functions with a body. 551 compile with everything they reach when checked as an entry (547 of the 1,021 that are not generic); 678 are themselves inside the subset. The 129 generic ones cannot be judged alone.
- Standard library, Rust builtins: 650. 387 can be called from native code (310 runtime symbols, 71 compiled inline, 6 trace option makers accepted and ignored). 263 cannot.
- Test corpora: 12,826 functions checked as entries, 6,946 compile with everything they reach. The conformance suite is 1,634 of 1,634, the speedtest `main`s 77 of 78, `baml_tests/baml_src` 3,678 of 8,611.
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
- [~] **Optional scalars inside containers.** An `int?`, `float?` or `bool?` is a pair of words. It works as a local, parameter, result, field and tuple element. It is refused as an array element (`cg/types.rs:266`), a map value (`274`), a future's value (`283`) and a type argument of a native (`cg/check.rs:547`): `list of unsupported elements value`, `map value`, `future value`. [stdlib 44 + 14 + 1, corpus 289 + 176 + 5 / 2,112]
  ```baml
  function main() -> int { let xs: int?[] = [1, null, 3]; xs.length() }
  // blocked user.main  list of unsupported elements value  _1
  function main() -> int { let f = spawn { if 1 > 0 { 3 } else { null } }; (await f) ?? 0 }
  // blocked user.main  spawn  of a plan with another layout
  ```
- [~] **Maps.** Keys are `string`, `int`, `bool`, an enum or `float` only (`cg/types.rs:171-176`); a value may not be `null` or an optional scalar. `map<K, int>` for a class `K` with its own `hash` is refused.
  ```baml
  class K { a: int }
  function main() -> int { let m: map<K, int> = {}; m.length() }
  // blocked user.main  map value  _2
  ```
- [~] **Unions.** A union of supported members is a tagged reference and works, with `match`. `union value` appears when a member has no native type (media, `type`, an optional-scalar list). [stdlib 91, corpus 447 / 2,208]
- [ ] **`never` as a declared type.** `result is a never value`, `never value`. A function declared `-> never` (`baml.sys.panic`, `baml.sys.exit`, `baml.http.Server.serve`) is refused when it is not inlined away. Called directly from `main` both inline and compile. [stdlib 7, corpus 52 + 37 / 331]
- [~] **Generics.** Code is compiled per type argument (`cg/program.rs:39-41`), every reference type sharing one copy (`cg/types.rs:158-164`). `type variable value (_N has no one type)`, `result is a type variable value` and `call with type arguments (a type without one type id)` appear when a type argument is not known where the call is written. [stdlib 113 + 40 + 1, corpus 976 + 234 + 95 / 2,971]. Part of this count is the census checking generic functions as entries, which no program does. Confirmed real cases:
  ```baml
  function biggest<T extends baml.ops.Compare>(xs: T[]) -> T {
    let best = xs[0]; for (let x in xs) { if x > best { best = x; } } best
  }
  function main() -> int { biggest([3, 9, 2]) }
  // blocked user.main  type variable value  _7 has no one type
  // blocked user.main  interface method call  gt: implemented in source for a type kept in a box

  class P { x: int }
  function main() -> int { let ps = [P { x: 3 }]; ps.sort_by_key((p: P) -> int { p.x }); ps[0].x }
  // blocked baml.Array.sort_by_key<unknown, int, _>  type variable value  _9 has no one type
  ```

### Calls

- [ ] **Defaulted arguments that are not constants, or are optional scalars.** `defaulted argument (of an int or float)` at the call (`cg/check.rs:776`), `(of an optional scalar)` in the callee (`cg/check.rs:1186`), bare at `cg/types.rs:1093`. An `int` or `float` has no bit pattern meaning "omitted" (`cg/types.rs:1128-1138`), so only a constant default works: the caller passes it. [stdlib 43, corpus 1,531 / 2,154]. Second largest kind. `openai.ResponsesClient.new` and every `@spec` projection hit it.
  ```baml
  function f(n: int, limit: int? = null) -> int { if n <= 0 { return 0; } 1 + f(n - 1) }
  function main() -> int { f(3) }
  // blocked user.main  defaulted argument  of an int or float
  // blocked user.f     defaulted argument  of an optional scalar

  function base(n: int) -> int { if n > 50 { base(n - 1) } else { n } }
  function f(n: int, by: int = base(3)) -> int { if n <= 0 { return 0; } by + f(n - 1, by = by) }
  function main() -> int { f(3) }
  // blocked user.main  defaulted argument  of an int or float
  ```
- [ ] **Globals (`const`).** `global read` (`cg/types.rs:1101`), `call of a non-function constant` (`cg/check.rs:496`). There is no initializer pass and no storage. [corpus 532 + 7 / 727]
  ```baml
  const LIMIT: int = 5;
  function main() -> int { LIMIT + 1 }
  // blocked user.main  global read
  ```
- [~] **Function values.** Closures over cells, plain functions and bound methods work. Refused: a generic function as a value, a function without source, virtual and generic function makers, a bound method with type arguments or on a non-reference receiver. `function value` (`cg/types.rs:1098-1100`, `cg/check.rs:1117-1119`), `bound method value` (`cg/check.rs:1363-1374`), `call of a value that is not a function` (`1694`), `closure that tests a type argument` (`1339`). [stdlib 21 + 19 + 12, corpus 631 + 50 + 12 + 59 + 33 / 2,311]
- [~] **Interface method dispatch.** Works for classes and enums that implement the method in BAML source with one layout. `interface method call` (`cg/check.rs:661-703`, `cg/dispatch.rs:227-322`, `cg/program.rs:421-492`), by detail, corpus functions blocked:
  - 201 `implemented in source for a type kept in a box`: an `int`, `float` or `bool` implements the interface and reaches it as an interface value.
  - 109 `implemented in Rust for a type only known when it runs` (`random_int`, `update`): a builtin behind dynamic dispatch that is not an `async` native of one-word arguments (`cg/program.rs:137-155`).
  - 73 `on a type with no implementation found`.
  - 55 `the language's own implementation` (`hash`, `to_json`, `to_string` the compiler synthesizes).
  - 29 `on an interface of type parameters` or `of a type parameter with no one type`, 13 `of elements with a compare of their own`, 4 `dispatched on a later argument`, 4 `a default body that tests its type argument`, 1 `implementations with different layouts`.

  [stdlib 13, corpus 474 / 517]
  ```baml
  interface Sized { function size(self) -> int throws never; }
  implements Sized for int { function size(self) -> int throws never { self } }
  class C { n: int
    implements Sized { function size(self) -> int throws never { self.n } } }
  function pick(i: int) -> Sized { if i % 2 == 0 { return C { n: i }; }; return 7; }
  function total(s: Sized, d: int) -> int { if d <= 0 { return s.size(); } total(s, d - 1) }
  function main() -> int { total(pick(1), 3) + total(pick(2), 3) }
  // blocked user.main  interface method call  size: implemented in source for a type kept in a box
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

- [~] **Type tests.** `type test` (`cg/check.rs:1472-1531`, `1676`). Works on a union against a class, enum, scalar or `null`. Refused, corpus functions: 625 `of a type without one type id` (a generic class instantiation, a `type` value, a map or array type inside the tested type), 60 `between array types`, 26 `of a literal, on a value of another type or of several`, 21 `of a type argument or an interface, on a value that is not a union`. [stdlib 64, corpus 668 / 2,360]
  ```baml
  function pick(n: int) -> int[] | string[] { if n == 0 { [1] } else { ["a"] } }
  function main() -> int { let v = pick(0); if v is int[] { 1 } else { 2 } }
  // blocked user.main  array construction where no array is expected  union
  // blocked user.main  type test  between array types
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

- [~] **Generic natives** take a type argument only as one word: not `null`, not an optional scalar, not a type with no native representation (`cg/check.rs:536-550`). `generic standard library native`, corpus functions: `Array.slice: called with other type arguments` 25, `Array._truncate`, `Map.has`, `Map.keys`, `Map.set`, `Array.clear`, `_is_primitive_array: at a type with no native representation` 17, `Map.index: on unknown` 3, `Map.get: at optional`, `_to_json_default: at null`, `json._to: at null`.
- [~] **`Array.join`**: section 1.
- [~] **Natives cannot take closures.** None of the 11 closure-taking `Array` methods has a signature. The inliner swaps in a BAML sibling (`baml_compiler2_mir/src/optimize/inline.rs:517-551`, `std/baml/containers.baml`): `_inlined_filter`, `_inlined_map`, `_inlined_some`, `_inlined_every`, `_inlined_find`, `_inlined_find_index`, `_inlined_reduce`, and `_driver_sort_by`, `_driver_find_last`, `_driver_find_last_index`, `_driver_flat_map`. `map`, `filter`, `reduce`, `sort_by` with a closure literal and `map` with a function parameter are confirmed to compile. A `map` whose result element is an optional scalar is refused (`list of unsupported elements value`).
- [~] **Defaulted `int` or `float` arguments** of a native: same rule as section 1 (`Constant::OmittedArg`, `cg/types.rs:1093`).
- [~] **A native behind interface dispatch** must be `async`, take only `int`s and references and return no `float` (`cg/program.rs:137-155`). Otherwise `interface method call (implemented in Rust for a type only known when it runs)`.
- [~] **A native cannot call back into BAML except through `scope.call_baml`**, used by `baml.json` hooks and `eq`. `to_string`, `hash` and `cmp` of user types are never called from Rust.
- [~] **`baml.json.parse` of an integer past 64 bits** throws natively, is a `bigint` on the interpreter (`differences.txt`, `json_parse_past_int`). Arrays and maps of scalars held as `unknown` do not serialize (`json_unknown_scalar_array`, `json_unknown_scalar_map`). Decode depth is capped at 128 (`json_to_deep`).
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
- [~] **Generics.** Monomorphized per scalar, shared per reference. `class Box<T>`, `unbox<T>`, generic closures confirmed. Gaps in section 1.
- [ ] **Interfaces with fields.** Absent. Section 1.
- [x] **Enums.** Plain variants, `match`, `==`, interpolation, map keys work. Enums carry no data in the language (`std` and the corpus declare none; not confirmed against the grammar).
- [~] **Unions and optional scalars.** Built (`cg/types.rs:90-99`). Gaps: optional scalars in containers, union arithmetic, `??` on `unknown`.
- [~] **Pattern matching and type tests.** Built for unions, literals, enums, tuples (`cg/narrow.rs`). Gaps: array types, generic instantiations, shape patterns on unions, truthiness.
- [~] **Tuples.** Built: construction, destructuring, as results (`cg/types.rs:74-77`, confirmed). Gaps: no JSON, a tuple with an unsupported element is `tuple value` [corpus 15 + 10 + 3].
- [~] **`bigint`.** Built: literals, arithmetic, `to_string`, 8 of 8 `Bigint` natives. Gaps: no JSON either way, no truthiness.
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
- [x] **Exit codes.** `baml.sys.exit(n)` throws `baml.panics.Exit`, which `baml-cli run` turns into the process status (`bex_engine/src/lib.rs:818`, `baml_cli/src/run_command.rs:1247`), and so does the native runtime when nothing in the entry task catches it (`baml_rt_uncaught`, `rt/unwind.rs`): no message, the `defer`s run, the errors no `await` took are still reported, and a code past an `i32` is the nearest one (so the OS sees its low 8 bits). An `Exit` in a task nothing awaits is an unhandled task error on both, status 1. Cases `sys_exit*` in `tests/conformance/sys.baml`.
- [x] **stdin.** `baml.io.input` works (ran it; also `a_program_prints_and_reads_its_own_streams`).
- [ ] **Signals.** No handler is installed anywhere in `rt/`. SIGINT and SIGTERM end the process at once: no `defer`, no finalizer, no flush. What `baml-cli run` does on SIGINT: not confirmed.
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
- [ ] **Death by signal** of the native program itself (SIGSEGV on the guard page, SIGINT while parked). The harness records it (`conformance.rs:50-60`) and no case produces it. `ProcessExit.signal` of a child is covered.
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
