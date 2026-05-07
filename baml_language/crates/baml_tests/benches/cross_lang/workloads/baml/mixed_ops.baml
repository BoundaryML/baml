class Point {
  x int
  y int
}

function main() -> int {
  let sum = 0;
  let s = "";
  let arr: int[] = [];
  for (let i = 0; i < 5000; i += 1) {
    sum += i * 3 - 1;
    s = s + "x";
    arr.push(i);
    let p = Point { x: i, y: i + 1 };
    sum += p.x + p.y;
    if i > 2500 { sum += 1; };
  };
  return sum + s.length() + arr.length();
}
