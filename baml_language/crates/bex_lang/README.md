# bex_lang

The language rules the bytecode VM (`bex_vm`) and ahead-of-time native code
(`bex_aot`) must agree on, written once. Both backends evaluate the same
BAML, so a divergence here is a bug in the language, not in a backend.
In this change only native code (through `bex_aot`) calls these functions;
the VM still has its own copies of each rule, and switching its builtins to
call this crate is a separate, VM-owned change. The "who calls it" column
names the VM sites that change then.

| Module | Rule | Who calls it |
|---|---|---|
| `int` | `+ - * / % -` on `Int63` fail with the language's panics; `<< >>` reject a negative count; the `*_message` helpers word `IntegerOverflow` and `NegativeBitShift` | the VM's cold arithmetic paths (`vm.rs`, `ops_math.rs`, `ops_bitwise.rs`, `root.rs`), native arithmetic |
| `float` | the total order, the `to_string` spelling, `itrunc`/`ifloor`/`iceil`/`iround` and the `to_int` behind them, with their `InvalidArgument` messages | `bex_vm::package_baml::float`, native `Float.i*` |
| `index` | negative subscripts count from the end: `resolve_index` for `[]`/`at`, `resolve_insert_index` for `insert`, `resolve_slice_bound` for `slice` | the VM's array, string and byte-array builtins and `LoadArrayElement`, native `array::get`/`set` |
| `Panic` | the catchable `baml.panics.*` classes as plain data: class name, fields, exit code, and `render_object`, the engine's `Class {field: value}` rendering | native `Thrown`, the VM's parity test |
| `Error` | the closed set a shared builtin fails with: a `Panic` or `InvalidArgument` | `bex_vm_types` converts it to `VmRustFnError`, `bex_aot` to `Thrown` |
| `clamp_exit_code` | `baml.sys.exit(code)` narrowed to a host `i32` | `baml_exec` (re-export) and native `Panic::exit_code` |

The crate depends on `baml_type` only, for `Int63`. It knows nothing of a heap, a VM
`Value` or a Rust handle: a panic here is data, and materializing it as a
BAML object (the VM) or carrying it as a `Thrown` (native) is each backend's
job. Everything must stay `wasm32`-safe and print nothing.
