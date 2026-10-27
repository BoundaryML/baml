class Acc { n: int }
function main() -> int {
  let lock = baml.sync.Lock.new();
  let acc = Acc { n: 0 };
  let add = () -> void { acc.n += 2; };
  for (let i = 0; i < 1000000; i += 1) { lock.with(add); };
  return acc.n;
}
