/// Internal: invoked only by compiler-synthesized host-callable wrapper
/// closures. Not directly callable by user code.
///
/// `T` is the expected return type, delivered to the native handler at
/// runtime via the type-arg channel. The handler validates both the
/// inbound `args` and the host's returned value against the declared
/// signature, raising `root.errors.HostCallable` on mismatch.
function call_host_value<T>(handle: HostValue, args: unknown[])
    -> T throws root.errors.HostCallable {
  $rust_io_function
}

/// Opaque builtin class. Construction is reserved to the bridge
/// boundary; users cannot construct or destructure it.
class HostValue {
  // intentionally empty; the value lives in the BexExternalValue
  // HostValue slot.
}
