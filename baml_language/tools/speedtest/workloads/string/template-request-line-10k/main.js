// The request line an HTTP client builds, as one backtick template with
// string and int holes, 10,000 times.
function main() {
  const names = ["apple", "pear", "plum", "fig", "kiwi"];
  const families = ["FR", "VG", "DY"];
  const limit = 25;
  const host = "api.example.com";
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i++) {
    seed = seed * 16807 % 2147483647;
    const org = families[i % 3];
    const item = names[seed % 5];
    const id = seed % 100000;
    const page = i % 50;
    const s = `GET /v1/orgs/${org}/items/${item}/${id}?page=${page}&limit=${limit}&sort=${item}&order=${org} host=${host} user=${item}-${id} attempt=${page} region=${org}`;
    total += s.length;
  }
  return total;
}
