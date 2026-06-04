/// Internal: invoked only by compiler-synthesized host-callable wrapper
/// closures. Not directly callable by user code.
///
/// `T` is the expected return type and `E` the declared error contract,
/// both delivered to the native handler at runtime via the type-arg channel
/// (`type_arg_0` / `type_arg_1`). The handler validates the host's returned
/// value against `T` today; `E` is plumbed end-to-end but not yet
/// consumed at the completion site — a future phase will reject host
/// throws that fall outside `E` as `baml.panics.HostContractViolation`. A
/// host value left untyped at the boundary erases `E` to `unknown`.
function call_host_value<T, E>(handle: HostValue, args: unknown[])
    -> T throws E {
  $rust_io_function
}

/// Opaque builtin class. Construction is reserved to the bridge
/// boundary; users cannot construct or destructure it.
class HostValue {
  // intentionally empty; the value lives in the BexExternalValue
  // HostValue slot.
}
