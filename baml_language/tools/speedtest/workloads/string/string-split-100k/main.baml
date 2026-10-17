function main() -> int {
  let count = 0;
  for (let i = 0; i < 100000; i += 1) {
    let s = "hello world foo bar baz qux";
    let parts = s.split(" ");
    count += parts.length();
  };
  return count;
}
