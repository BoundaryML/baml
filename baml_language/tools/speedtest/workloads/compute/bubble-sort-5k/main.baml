function main() -> int {
  let arr: int[] = [];
  for (let i = 0; i < 5000; i += 1) { arr.push(5000 - i); };
  for (let i = 0; i < arr.length(); i += 1) {
    for (let j = 0; j < arr.length() - i - 1; j += 1) {
      if arr[j] > arr[j + 1] {
        let tmp = arr[j];
        arr[j] = arr[j + 1];
        arr[j + 1] = tmp;
      };
    };
  };
  return arr[0] + arr[4999];
}
