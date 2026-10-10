class State { message: string }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(state: State) -> string { baml.json.to_string(state) }
