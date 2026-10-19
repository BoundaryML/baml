async function sumRange(start, end) {
  let s = 0;
  for (let i = start; i < end; i++) s += i;
  return s;
}
async function main() {
  const [a, b, c, d] = await Promise.all([
    sumRange(0, 250000),
    sumRange(250000, 500000),
    sumRange(500000, 750000),
    sumRange(750000, 1000000),
  ]);
  return a + b + c + d;
}
