// Generate and sort (from the native-bench generate_sort case): 8,192 numbers
// from a multiplicative generator, sorted with the library sort and
// serialized to JSON.
function main() -> int {
  let a: int[] = [];
  let value = 42;
  for (let i = 0; i < 8192; i += 1) {
    value = value * 48271 % 1000000007;
    a.push(value);
  }
  a.sort();
  let out = baml.json.to_string(a);
  let h = 0;
  for (let c in out.to_code_points()) { h = (h * 31 + c) % 1000000007; }
  h + out.length()
}
