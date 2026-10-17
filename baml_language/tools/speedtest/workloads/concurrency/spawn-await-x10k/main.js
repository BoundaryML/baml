async function main() {
  let s = 0;
  for (let i = 0; i < 10000; i++) s += await Promise.resolve(1);
  return s;
}
