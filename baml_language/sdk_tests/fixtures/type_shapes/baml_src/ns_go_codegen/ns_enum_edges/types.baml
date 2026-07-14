// Exhaustive Go enum composition fixture. The generated representation is a
// named string type, while generated codecs validate this closed variant set
// before values cross the FFI boundary in either direction.
enum ResponseState {
  pending_review,
  accepted,
  HTTP_error,
}

class EnumMatrix {
  state ResponseState
  optional_state ResponseState?
  states ResponseState[]
  optional_states ResponseState?[]
  states_by_name map<string, ResponseState>
  optional_states_by_name map<string, ResponseState?>
  optional_list ResponseState[]?
  optional_map map<string, ResponseState>?
}

function round_trip_state(value: ResponseState) -> ResponseState {
  value
}

function round_trip_optional_state(value: ResponseState?) -> ResponseState? {
  value
}

function round_trip_matrix(value: EnumMatrix) -> EnumMatrix {
  value
}

function defaulted_state(value: ResponseState = ResponseState.pending_review) -> ResponseState {
  value
}

// These declarations deliberately project to the same package-scope Go base
// name as each other. Typed collision allocation must preserve all three.
enum Collision {
  item,
}

class CollisionItem {}

function collision_item() -> string {
  "function"
}
