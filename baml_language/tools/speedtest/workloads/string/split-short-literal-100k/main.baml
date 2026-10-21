function main() -> int {
  let s = "hello world foo bar baz qux";
  let delim = " ";
  let count = 0;
  for (let i = 0; i < 100000; i += 1) {
    let parts = s.split(delim);
    count += parts.length();
  };
  return count;
}
