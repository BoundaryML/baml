/// Internal: invoked only by compiler-synthesized host-callable wrapper
/// closures. Not directly callable by user code.
///
/// `T` is the expected return type and `E` the declared error contract,
/// both delivered to the native handler at runtime via the type-arg channel
/// (`type_arg_0` / `type_arg_1`). The completion site validates the host's
/// returned value against `T` and the thrown value against `E`: a return
/// that doesn't inhabit `T` and a throw that isn't a subtype of `E` both
/// become `baml.panics.HostContractViolation` (uncatchable). An on-contract
/// throw rides the VM's normal exception unwinder. A host value left
/// untyped at the boundary erases `E` to `unknown`, which accepts any
/// thrown value — including an opaque `baml.errors.HostCallable`.
///
/// `args` is a two-element pack the VM builds in `host_closure_call_sysop`:
/// `[positional_required_args, { optional_name: value }]`. The VM has already
/// split the call by the callable's declared params (using the captured
/// `Object::HostClosure`), dropping omitted optionals — so each bridge applies
/// its own calling convention (TS `$opts`, Python kwargs) without needing the
/// callee type on the wire.
function call_host_value<T, E>(handle: Value, args: unknown[]) -> T throws E {
    $rust_io_function
}

/// Opaque builtin class. Construction is reserved to the bridge
/// boundary; users cannot construct or destructure it.
class Value {
    // intentionally empty; the value lives in the BexExternalValue
    // Value slot.
}
