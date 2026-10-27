// The request line an HTTP client builds, as one backtick template with
// string and int holes, 10,000 times.
function main() -> int {
  let names = ["apple", "pear", "plum", "fig", "kiwi"];
  let families = ["FR", "VG", "DY"];
  let limit = 25;
  let host = "api.example.com";
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i += 1) {
    seed = seed * 16807 % 2147483647;
    let org = families[i % 3];
    let item = names[seed % 5];
    let id = seed % 100000;
    let page = i % 50;
    let s = `GET /v1/orgs/${org}/items/${item}/${id}?page=${page}&limit=${limit}&sort=${item}&order=${org} host=${host} user=${item}-${id} attempt=${page} region=${org}`;
    total += s.length();
  }
  total
}
