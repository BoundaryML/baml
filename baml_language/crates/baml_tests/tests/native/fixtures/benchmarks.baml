// Native backend fixture: the shape of the benchmark workloads.

class State { count: int, seed: int }

function prepare(raw: string) -> State {
    baml.json.deserialize<State>(raw)
}

function leaf(value: int) -> int { value + 1 }

function run(state: State) -> string {
    let value = state.seed;
    let i = 0;
    while (i < state.count) { value = leaf(value); i += 1; }
    value.to_string()
}

function sort_kernel(xs: int[]) -> int[] {
    let n = xs.length();
    let i = 1;
    while (i < n) {
        let key = xs[i];
        let j = i - 1;
        while (j >= 0 && xs[j] > key) {
            xs[j + 1] = xs[j];
            j = j - 1;
        }
        xs[j + 1] = key;
        i = i + 1;
    }
    xs
}

function sorted_checksum(n: int) -> int {
    let xs: int[] = [];
    let i = 0;
    while (i < n) {
        xs.push((i * 7919) % 1009);
        i = i + 1;
    }
    let ys = sort_kernel(xs);
    let acc = 0;
    for (let y in ys) {
        acc = (acc * 31 + y) % 1000003;
    }
    acc
}

function pipeline(raw: string) -> string {
    run(prepare(raw))
}
