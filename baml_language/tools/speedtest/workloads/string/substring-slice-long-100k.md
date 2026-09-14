# string::substring slice long 100k

## eval-setup
```py
import json
chunk = (
    "the quick brown fox jumps over the lazy dog and runs through the meadow "
    "while the sun sets behind the distant mountains casting long shadows "
    "across the rolling hills and the gentle breeze carries the scent of "
    "wildflowers and freshly cut grass through the warm summer air as birds "
    "sing in the trees and the world feels at peace with itself in this "
    "perfect moment of stillness and beauty that seems to stretch on forever "
    "without end or care for the troubles of yesterday or tomorrow only the "
    "present moment matters now"
)
src = "###".join([chunk] * 10)
baml_src = json.dumps(src)
py_src = repr(src)
js_src = json.dumps(src)
```

## BAML
```baml
function main() -> int {
  let s = $$baml_src;
  let total = 0;
  for (let i = 0; i < 100000; i += 1) {
    let sub = s.slice(100, 400);
    total += sub.length();
  };
  return total;
}
```

## Python
```python
s = $$py_src
t = 0
for _ in range(100000):
    t += len(s[100:400])
print(t)
```

## Typescript
```ts
const s = $$js_src;
let t = 0;
for(let i=0;i<100000;i++) t += s.substring(100, 400).length;
console.log(t);
```
