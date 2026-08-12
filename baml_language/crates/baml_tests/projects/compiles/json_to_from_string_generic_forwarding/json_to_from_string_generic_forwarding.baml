// Phase 5 fixture: generic forwarding through `from_string<T>`.
// MIR snapshot must show `LoadType(TypeArgRef(0))` for the inner call.

function fetch_as<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError {
  baml.json.from_string<T>(s)
}
