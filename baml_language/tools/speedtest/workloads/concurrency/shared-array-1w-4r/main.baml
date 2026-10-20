function write_n(arr: int[], n: int) -> int {
  let i = 0;
  while i < n { arr.push(i); i += 1; };
  0
}
function read_n(arr: int[], n: int) -> int {
  let i = 0;
  let acc = 0;
  while i < n {
    let len = arr.length();
    if len > 0 { acc = acc + arr[0]; };
    i += 1;
  };
  acc
}
function main() -> int {
  let arr = [0];
  let w = spawn { write_n(arr, 50000) };
  let r1 = spawn { read_n(arr, 12500) };
  let r2 = spawn { read_n(arr, 12500) };
  let r3 = spawn { read_n(arr, 12500) };
  let r4 = spawn { read_n(arr, 12500) };
  (await w) + (await r1) + (await r2) + (await r3) + (await r4);
  arr.length()
}
