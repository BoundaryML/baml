# baml_builtins2_codegen

Build-time code generation for the Rust-backed builtins declared in the
`baml_builtins2` stdlib (`$rust_function`, `$rust_io_function`). It emits the
interpreter's `Value` glue and dispatch tables (`codegen.rs`), the sysop
pipeline (`codegen_io.rs`), and adapters on the shared object format
(`codegen_adapters.rs`).

## Adapters on the shared object format

The shared runtime stores every field and element in one of three slot kinds:
a raw 8-byte word (`int`, `float`, `bool`), an 8-byte object pointer, or a
16-byte tagged slot (unions, optionals, `unknown`, interpreter generics). The
ABI that adapters and their bodies are written against lives in
`bex_vm_types::layout`: the slot vocabulary (`Kind`, `ElemKind`, `SlotKind`),
the three slot representations (`RawWord`, `ObjPtr`, `Tagged`), and heap
traits (`Descriptors`, `Lists<E>`, `Maps<V>`, `Strings`, `ObjectModel`).

Every native declaration has a row in `src/triage_table.rs` giving its census
group and adapter form. `triage::check_coverage` (run by the crate's tests)
fails when a declaration has no row or a row names no declaration.

Each adapter builtin has exactly one body, in place in its `package_baml`
file (`array.rs`, `map.rs`, `string.rs`, `ops.rs`), generic over the slot
representation `E: Elem` and re-exported as `package_baml::bodies`. The
interpreter runs it at `E = Value` (today's tagged word) through
`package_baml::value_model::ValueModel`, which implements the layout traits
over today's `Value`/`Object`/`LockedContainer` heap. For these builtins
`codegen.rs` emits `__glue_{method}` (and its `$local` twin) that extracts
arguments with the same errors as the old `Value` glue, calls the body, and
requires no `Value` trait method. A builtin that may yield (`Map` lookups by a
non-string key, `join` of elements with `to_string` overrides) gets
`Fault::NeedsVm` from its body and falls back to `bodies::{fn_name}_yielding`,
the interpreter's continuation path.

From the declaration, the generator also emits two adapter forms for
compiled code, over the same bodies:

- **Generic** (`AdapterForm::Generic`): `{fn_name}(rt, receiver, ...)`. It
  reads the receiver's descriptor (`Shape`), converts generic arguments from
  their universal `Tagged` form to the receiver's slot representation, and
  branches to the body monomorphized at that slot kind. Compiled code uses it
  when the element type is not known statically. The per-kind functions it
  calls stay private.
- **Specialized** (`AdapterForm::Specialized`): the generic adapter plus
  public per-kind entry points `{fn_name}_raw8`, `{fn_name}_ptr` and
  `{fn_name}_tagged16` that take the slot representation directly and read
  no descriptor. Compiled code calls these when it knows the type. String
  receivers have one slot kind, so a specialized string builtin exports only
  `{fn_name}_ptr`.

Rows with `ValueGlue` keep today's `Value` glue unchanged (scalars, strings,
sysops). `Pending(reason)` rows touch object layout but have no adapter yet;
`PendingReason::description` says what unblocks each one.

Adapters return `Result<_, Fault>`. `Fault::NeedsVm` means the adapter cannot
finish without interpreter machinery (a non-string map key whose hash may run
user code, an element whose `to_string` may be overridden); the caller then
takes the `Value` glue path.

`bex_vm/build.rs` writes the adapters to `adapters_generated.rs`, which is
included in `bex_vm::adapters::generated`. `bex_vm/src/adapters/tests.rs`
checks every generated adapter against the interpreter's glue on receivers of
each slot kind (a test-only typed view of `ValueModel` reports raw and pointer
slots), and pins the interpreter's outputs for these builtins in
`vm_glue_golden.txt`, recorded from the `Value` bodies they replaced.

### Moving a builtin between forms

- **Pending or ValueGlue to Generic**: rewrite the builtin's existing `Value`
  body in place as `{fn_name}` with the signature the generator derives from
  the declaration, delete the old trait method, and add it to
  `package_baml::bodies`. Receiver first, then parameters in ABI types: `int`
  is `i64`, `string` and lists are `ObjPtr`, the receiver's element generic is
  `E`, `T?` is `Option<E>`, a map key is an `ObjPtr` string; the result is
  `Result<_, Fault>`. If the builtin may yield, also provide
  `{fn_name}_yielding(vm, args, locality)` for `Fault::NeedsVm`. Change the
  row's form to `Generic`, add a differential case in
  `bex_vm/src/adapters/tests.rs` (`every_generated_adapter_has_a_differential_case`
  fails until you do), and record its interpreter outputs in the golden before
  you replace the body. Signatures the generator cannot map fail the build
  with an `AdapterGenError` naming the type. Builtins that call back into BAML
  (`Pending(Callback)`) stay on their continuation: a synchronous body cannot
  replace it until the interpreter can call bytecode from a native.
- **Generic to Specialized**: change the form to `Specialized`. The per-kind
  functions become public; nothing else changes.
- **Back to ValueGlue or Pending**: change the form and give the builtin a
  `Value` trait method again; the generated glue then requires it.

Only `Array`, `Map` and `String` instance methods have adapter shapes today.
Supporting another receiver means adding a `ReceiverShape` in
`codegen_adapters.rs` and a receiver check on `layout::Shape`.
