/// Emit a custom event with the given name and data.
/// The event will be associated with the current span context.
///
/// Note: The compiler special-cases `baml.events.send` to emit the `SendEvent`
/// VM instruction directly, so the `$rust_function` body here is a fallback
/// that is registered but never invoked at runtime.
//baml:mut_vm
function send(event_name: string, data: map<string, unknown>) -> null {
  $rust_function
}
