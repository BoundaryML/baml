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
///
/// `handle` is the host closure being called: the VM passes it, never a
/// program.
function _call_host_value<T, E>(
    handle: reflect.AnyFunction<Returns = T, Throws = E>,
    args: unknown[],
) -> T throws E {
    $rust_io_function
}

/// A value the host owns, held by the host's reference to it: the host that
/// sent it resolves it back to its own object, so it round-trips as itself.
/// A program holds and passes it, but cannot build or look into one.
class Value {
    @@rust_type("HostValue")
}
