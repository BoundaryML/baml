// Three-way quicksort with an explicit stack (from the native-bench quick_sort
// case): 1,024 integers in four orders (random, sorted, reversed, many
// duplicates), each copied, sorted in BAML code and serialized to JSON.
// setup() builds the four inputs once, untimed.
function setup() -> int[][] {
  let seed = 7;
  let random: int[] = [];
  let dups: int[] = [];
  for (let i = 0; i < 1024; i += 1) {
    seed = seed * 16807 % 2147483647;
    random.push(seed % 100000);
    dups.push(seed % 16);
  }
  let sorted: int[] = [];
  let reversed: int[] = [];
  for (let i = 0; i < 1024; i += 1) {
    sorted.push(i * 3);
    reversed.push((1023 - i) * 3);
  }
  [random, sorted, reversed, dups]
}

function quick_sort(values: int[]) -> int[] {
  let a: int[] = [];
  for (let v in values) { a.push(v); }
  let lows: int[] = [0];
  let highs: int[] = [a.length() - 1];
  let top = 1;
  while (top > 0) {
    top -= 1;
    let lo = lows[top];
    let hi = highs[top];
    if (lo < hi) {
      let pivot = a[lo + (hi - lo) / 2];
      let lt = lo;
      let i = lo;
      let gt = hi;
      while (i <= gt) {
        if (a[i] < pivot) {
          let v = a[lt]; a[lt] = a[i]; a[i] = v; lt += 1; i += 1;
        } else if (a[i] > pivot) {
          let v = a[gt]; a[gt] = a[i]; a[i] = v; gt -= 1;
        } else {
          i += 1;
        }
      }
      if (top == lows.length()) { lows.push(lo); highs.push(lt - 1); }
      else { lows[top] = lo; highs[top] = lt - 1; }
      top += 1;
      if (top == lows.length()) { lows.push(gt + 1); highs.push(hi); }
      else { lows[top] = gt + 1; highs[top] = hi; }
      top += 1;
    }
  }
  a
}

function run(inputs: int[][]) -> int {
  let check = 0;
  for (let input in inputs) {
    let out = baml.json.stringify(quick_sort(input));
    let h = 0;
    for (let c in out.to_code_points()) { h = (h * 31 + c) % 1000000007; }
    check = (check + h + out.length()) % 1000000007;
  }
  check
}

function main() -> int {
  run(setup())
}
