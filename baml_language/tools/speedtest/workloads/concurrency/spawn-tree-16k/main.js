async function tree(depth) {
  if (depth === 0) return 1;
  const left = tree(depth - 1);
  const right = tree(depth - 1);
  return (await left) + (await right) + 1;
}
async function main() {
  return tree(13);
}
