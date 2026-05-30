# string::contains medium literal 100k

## eval-setup
```py
import json
chunk = "the quick brown fox jumps over the lazy dog and runs through the meadow"
src = "|".join([chunk] * 8)
needle = "meadow"
baml_src = json.dumps(src)
baml_needle = json.dumps(needle)
py_src = repr(src)
py_needle = repr(needle)
js_src = json.dumps(src)
js_needle = json.dumps(needle)
```

## BAML
```baml
function main() -> int {
  let s = $$baml_src;
  let needle = $$baml_needle;
  let count = 0;
  for (let i = 0; i < 100000; i += 1) {
    if s.includes(needle) { count += 1; };
  };
  return count;
}
```

## Python
```python
s = $$py_src
n = $$py_needle
c = 0
for _ in range(100000):
    if n in s: c += 1
print(c)
```

## Typescript
```ts
const s = $$js_src;
const n = $$js_needle;
let c = 0;
for(let i=0;i<100000;i++) if(s.includes(n)) c += 1;
console.log(c);
```
