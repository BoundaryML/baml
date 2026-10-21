function main() {
  const s = "hello world foo bar baz qux";
  const delim = " ";
  let c = 0;
  for(let i=0;i<100000;i++) c += s.split(delim).length;
  return c;
}
