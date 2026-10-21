async function task(i) { return i * 2; }
async function main() {
  const fs = [];
  for (let i = 0; i < 100; i++) fs.push(task(i));
  let s = 0;
  for (const f of fs) s += await f;
  return s;
}
