# string::split long literal 1k

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
delim = "###"
baml_src = json.dumps(src)
baml_delim = json.dumps(delim)
py_src = repr(src)
py_delim = repr(delim)
js_src = json.dumps(src)
js_delim = json.dumps(delim)
```

## BAML
```baml
function main() -> int {
  let s = $$baml_src;
  let delim = $$baml_delim;
  let count = 0;
  for (let i = 0; i < 1000; i += 1) {
    let parts = s.split(delim);
    count += parts.length();
  };
  return count;
}
```

## Python
```python
s = $$py_src
delim = $$py_delim
c = 0
for _ in range(1000):
    c += len(s.split(delim))
print(c)
```

## Typescript
```ts
const s = $$js_src;
const delim = $$js_delim;
let c = 0;
for(let i=0;i<1000;i++) c += s.split(delim).length;
console.log(c);
```
