function main() -> int {
  let s = "";
  for (let i = 0; i < 5000; i += 1) {
    s = s + "hello";
  };
  return s.length();
}
