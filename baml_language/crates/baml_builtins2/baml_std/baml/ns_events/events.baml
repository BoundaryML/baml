/// Emit a custom event with the given name and data.
/// The event will be associated with the current span context.
function send(event_name: string, data: map<string, unknown>) -> void {
  $compiler_intrinsic
}
