function nap() {
  return new Promise((resolve) => setTimeout(() => resolve(1), 100));
}
async function tree(depth) {
  if (depth === 0) return nap();
  const left = tree(depth - 1);
  const right = tree(depth - 1);
  return (await left) + (await right) + 1;
}
async function main() {
  return tree(13);
}
