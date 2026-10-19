async function nap() {
  await new Promise(r => setTimeout(r, 200));
  return 1;
}
async function main() {
  const [a, b, c] = await Promise.all([nap(), nap(), nap()]);
  return a + b + c;
}
