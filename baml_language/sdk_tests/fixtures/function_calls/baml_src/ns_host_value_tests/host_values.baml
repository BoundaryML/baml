// A host's own value (`baml.host.Value`) crosses as the host's key to it and
// comes back as the same value.

/// Hands a host's own value back as it came.
function echo_host_value(value: baml.host.Value) -> baml.host.Value {
  value
}

/// Hands a host's own value back twice, in a list.
function twice_host_value(value: baml.host.Value) -> baml.host.Value[] {
  [value, value]
}

/// Whether two host values are the same value.
function same_host_value(a: baml.host.Value, b: baml.host.Value) -> bool {
  a == b
}
