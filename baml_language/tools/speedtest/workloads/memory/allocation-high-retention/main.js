class Cell { constructor(value, payload) { this.value = value; this.payload = payload; } }
function main() {
  const batches = 128, width = 2048, retain = 64;
  const retained = [];
  let checksum = 0;
  for (let batch = 0; batch < batches; batch++) {
    const cells = [];
    for (let i = 0; i < width; i++) {
      const cell = new Cell(batch + i, [i, i + 1, i + 2, i + 3]);
      const alias = cell;
      alias.value += 1;
      alias.payload[0] += 1;
      cells.push(cell);
    }
    for (const cell of cells) checksum += cell.value + cell.payload[0];
    if (retained.length < retain) retained.push(cells); else retained[batch % retain] = cells;
  }
  let liveSum = 0;
  for (const cells of retained) for (const cell of cells) liveSum += cell.value + cell.payload[0];
  return `${checksum},${liveSum}`;
}
