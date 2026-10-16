class State { values: int[] }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(state: State) -> string {
    let a: int[] = [];
    for (let v in state.values) { a.push(v); }
    let lows: int[] = [0]; let highs: int[] = [a.length() - 1]; let top = 1;
    while (top > 0) {
        top -= 1; let lo = lows[top]; let hi = highs[top];
        if (lo < hi) {
            let pivot = a[lo + (hi - lo) / 2];
            let lt = lo; let i = lo; let gt = hi;
            while (i <= gt) {
                if (a[i] < pivot) { let v = a[lt]; a[lt] = a[i]; a[i] = v; lt += 1; i += 1; }
                else if (a[i] > pivot) { let v = a[gt]; a[gt] = a[i]; a[i] = v; gt -= 1; }
                else { i += 1; }
            }
            if (top == lows.length()) { lows.push(lo); highs.push(lt - 1); }
            else { lows[top] = lo; highs[top] = lt - 1; }
            top += 1;
            if (top == lows.length()) { lows.push(gt + 1); highs.push(hi); }
            else { lows[top] = gt + 1; highs[top] = hi; }
            top += 1;
        }
    }
    baml.json.to_string(a)
}
