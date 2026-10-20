function main() {
  const a = [];
  let value = 42;
  for (let i = 0; i < 8192; i++) { value = value * 48271 % 1000000007; a.push(value); }
  a.sort((x, y) => x - y);
  const out = JSON.stringify(a);
  let h = 0;
  for (let j = 0; j < out.length; j++) h = (h * 31 + out.charCodeAt(j)) % 1000000007;
  return h + out.length;
}
