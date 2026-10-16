class State { values: int[], variant: string }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(state: State) -> string {
    let values = state.values;
    if (state.variant == "build") {
        values = [];
        let j = 0;
        while (j < state.values.length()) {
            values.push(state.values[j]);
            j += 1;
        }
    }
    let total = 0;
    if (state.variant == "indexed") {
        let i = 0;
        while (i < values.length()) { total += values[i]; i += 1; }
    } else {
        for (let value in values) { total += value; }
    }
    total.to_string()
}
