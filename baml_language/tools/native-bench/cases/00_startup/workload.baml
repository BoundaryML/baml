class State { value: int }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(state: State) -> string { state.value.to_string() }
