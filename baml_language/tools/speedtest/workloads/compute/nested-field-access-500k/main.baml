class Inner { value: int }
class Middle { inner: Inner }
class Outer { middle: Middle }
function main() -> int {
  let s = 0;
  for (let i = 0; i < 500000; i += 1) {
    let o = Outer { middle: Middle { inner: Inner { value: i } } };
    s += o.middle.inner.value;
  };
  return s;
}
