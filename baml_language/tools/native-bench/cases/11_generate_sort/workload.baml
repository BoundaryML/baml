class State { count: int seed: int }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(s: State) -> string {
 let a: int[] = []; let value = s.seed;
 for (let i = 0; i < s.count; i += 1) { value = (value * 48271) % 1000000007; a.push(value); }
 a.sort(); baml.json.to_string(a)
}
