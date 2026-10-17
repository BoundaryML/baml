function main() -> int {
  let grid: int[][] = [];
  for (let i = 0; i < 1000; i += 1) {
    let row: int[] = [];
    for (let j = 0; j < 100; j += 1) { row.push(i * 100 + j); };
    grid.push(row);
  };
  let s = 0;
  for (let row in grid) {
    for (let val in row) { s += val; };
  };
  return s;
}
