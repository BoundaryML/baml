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

Each builtin's body is written once, in `bex_vm/src/adapters/bodies.rs`,
generic over the slot representation `E: Elem`. From the declaration, the
generator emits two forms:

- **Generic** (`AdapterForm::Generic`): `{fn_name}(rt, receiver, ...)`. It
  reads the receiver's descriptor (`Shape`), converts generic arguments from
  their universal `Tagged` form to the receiver's slot representation, and
  branches to the body monomorphized at that slot kind. The interpreter
  always uses this form; compiled code uses it when the element type is not
  known statically. The per-kind functions it calls stay private.
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
included in `bex_vm::adapters::generated`. `bex_vm::adapters::vm_model::VmModel`
implements the ABI over today's `Value`/`Object` heap, and
`bex_vm/src/adapters/tests.rs` checks every generated adapter against the
`Value` glue on receivers of each slot kind. Nothing in the interpreter
dispatches to adapters yet.

### Moving a builtin between forms

- **Pending or ValueGlue to Generic**: write `bodies::{fn_name}` with the
  signature the generator derives from the declaration. Receiver first, then
  parameters in ABI types: `int` is `i64`, `float` is `f64`, `string` and
  lists are `ObjPtr`, the receiver's element generic is `E`, `T?` is
  `Option<E>`, a map key is an `ObjPtr` string, a callback is
  `&mut dyn FnMut(&mut R, E) -> Result<Tagged, Fault>`, and a returned list of
  a method generic adds a trailing `out: ListOf`. Change the row's form to
  `Generic` and add a differential case in `bex_vm/src/adapters/tests.rs`
  (`every_generated_adapter_has_a_differential_case` fails until you do).
  Signatures the generator cannot map fail the build with an
  `AdapterGenError` naming the type.
- **Generic to Specialized**: change the form to `Specialized`. The per-kind
  functions become public; nothing else changes.
- **Back to ValueGlue or Pending**: change the form. The adapter disappears
  and the `Value` glue is untouched either way.

Only `Array`, `Map` and `String` instance methods have adapter shapes today.
Supporting another receiver means adding a `ReceiverShape` in
`codegen_adapters.rs` and a matching receiver check in
`bex_vm/src/adapters/support.rs`.
