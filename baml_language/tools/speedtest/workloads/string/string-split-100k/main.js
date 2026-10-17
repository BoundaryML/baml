function main() {
  let c=0;
  for(let i=0;i<100000;i++){c+="hello world foo bar baz qux".split(" ").length}return c;
}
