function main() -> int {
  let s = "";
  for (let i = 0; i < 10000; i += 1) {
    s = s + "x";
  };
  return s.length();
}
