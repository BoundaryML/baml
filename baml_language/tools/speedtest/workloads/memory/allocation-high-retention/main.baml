// Allocation under retention (from the native-bench allocation_retention
// case): 128 batches of 2,048 small objects, each with an int array; the last
// 64 batches stay alive, so the collector or reference counts do real
// work as the survivors churn.
class Cell {
  value: int
  payload: int[]
}

function main() -> string {
  let batches = 128;
  let width = 2048;
  let retain = 64;
  let retained: Cell[][] = [];
  let checksum = 0;
  let batch = 0;
  while (batch < batches) {
    let cells: Cell[] = [];
    let i = 0;
    while (i < width) {
      let cell = Cell { value: batch + i, payload: [i, i + 1, i + 2, i + 3] };
      let alias = cell;
      alias.value += 1;
      alias.payload[0] += 1;
      cells.push(cell);
      i += 1;
    }
    for (let cell in cells) { checksum += cell.value + cell.payload[0]; }
    if (retained.length() < retain) { retained.push(cells); }
    else { retained[batch % retain] = cells; }
    batch += 1;
  }
  let live_sum = 0;
  for (let cells in retained) {
    for (let cell in cells) { live_sum += cell.value + cell.payload[0]; }
  }
  checksum.to_string() + "," + live_sum.to_string()
}
