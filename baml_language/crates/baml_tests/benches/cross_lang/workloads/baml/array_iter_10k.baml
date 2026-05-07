function build_array() -> int[] {
  let arr: int[] = [];
  for (let i = 0; i < 10000; i += 1) {
    arr.push(i);
  };
  return arr;
}

function main() -> int {
  let arr = build_array();
  let s = 0;
  for (let x in arr) {
    s += x;
  };
  return s;
}
