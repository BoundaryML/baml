// Exhaustive Go type-alias fixture. Alias declarations are emitted for users
// who import them directly, while signatures and wire codecs transparently
// lower aliases to the same canonical Go type as the underlying BAML shape.
type Text = string
type TextChain = Text
type Count = int
type Ratio = float
type Flag = bool
type BigNumber = bigint
type Key = string
type State = root.go_codegen.enum_edges.ResponseState
type OptionalState = root.go_codegen.enum_edges.ResponseState?
type States = root.go_codegen.enum_edges.ResponseState[]
type StatesByName = map<string, root.go_codegen.enum_edges.ResponseState>
type StatesByKey = map<Key, root.go_codegen.enum_edges.ResponseState>
type OptionalStates = root.go_codegen.enum_edges.ResponseState[]?
type OptionalStatesByName = map<string, root.go_codegen.enum_edges.ResponseState>?

function round_trip_text(value: TextChain) -> TextChain {
  value
}

function round_trip_optional_text(value: TextChain?) -> TextChain? {
  value
}

function round_trip_optional_state(value: OptionalState?) -> OptionalState? {
  value
}

function round_trip_optional_big_number(value: BigNumber?) -> BigNumber? {
  value
}

function round_trip_states(value: States) -> States {
  value
}

function round_trip_states_by_key(value: StatesByKey) -> StatesByKey {
  value
}

function round_trip_optional_states(value: OptionalStates) -> OptionalStates {
  value
}

function defaulted_state(value: State = root.go_codegen.enum_edges.ResponseState.pending_review) -> State {
  value
}

// These declarations deliberately normalize to one Go package-scope base.
// The typed name allocator must retain all three declarations deterministically.
type CollisionItem = string

enum Collision {
  item,
}

function collision_item() -> string {
  "function"
}

// Non-recursive union aliases are transparent and supported. Recursive aliases
// remain omitted until Go receives a dedicated recursive representation.
type Unsupported = string | int

function unsupported_alias(value: Unsupported) -> Unsupported {
  value
}
