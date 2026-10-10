class State { count: int, seed: int }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function leaf(value: int) -> int { value + 1 }
function run(state: State) -> string {
    let value = state.seed;
    let i = 0;
    while (i < state.count) { value = leaf(value); i += 1; }
    value.to_string()
}
