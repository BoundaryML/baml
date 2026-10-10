class State { values: int[] }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(state: State) -> string {
    let a: int[] = [];
    let b: int[] = [];
    for (let value in state.values) { a.push(value); b.push(0); }
    let width = 1;
    let n = a.length();
    while (width < n) {
        let lo = 0;
        while (lo < n) {
            let mid = lo + width;
            if (mid > n) { mid = n; }
            let hi = lo + 2 * width;
            if (hi > n) { hi = n; }
            let i = lo;
            let j = mid;
            let k = lo;
            while (k < hi) {
                if (i < mid && (j >= hi || a[i] <= a[j])) {
                    b[k] = a[i]; i += 1;
                } else { b[k] = a[j]; j += 1; }
                k += 1;
            }
            lo += 2 * width;
        }
        let tmp = a; a = b; b = tmp;
        width *= 2;
    }
    baml.json.to_string(a)
}
