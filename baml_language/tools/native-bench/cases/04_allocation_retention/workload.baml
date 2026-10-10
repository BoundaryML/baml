class Cell { value: int, payload: int[] }
class State { batches: int, width: int, retain: int }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
function run(s: State) -> string {
    let retained: Cell[][] = [];
    let checksum = 0;
    let batch = 0;
    while (batch < s.batches) {
        let cells: Cell[] = [];
        let i = 0;
        while (i < s.width) {
            let cell = Cell { value: batch + i, payload: [i, i + 1, i + 2, i + 3] };
            let alias = cell;
            alias.value += 1;
            alias.payload[0] += 1;
            cells.push(cell);
            i += 1;
        }
        for (let cell in cells) { checksum += cell.value + cell.payload[0]; }
        if (s.retain > 0) {
            if (retained.length() < s.retain) { retained.push(cells); }
            else { retained[batch % s.retain] = cells; }
        }
        batch += 1;
    }
    let live_sum = 0;
    for (let cells in retained) {
        for (let cell in cells) { live_sum += cell.value + cell.payload[0]; }
    }
    baml.json.to_string([checksum, live_sum])
}
