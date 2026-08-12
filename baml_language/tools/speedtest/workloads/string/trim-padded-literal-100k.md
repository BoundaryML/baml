# string::trim padded literal 100k

## eval-setup
```py
import json
src = (
    " " * 30
    + "the quick brown fox jumps over the lazy dog and runs through the meadow with great enthusiasm"
    + " " * 30
)
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
    let t = s.trim();
    total += t.length();
  };
  return total;
}
```

## Python
```python
s = $$py_src
t = 0
for _ in range(100000):
    t += len(s.strip())
print(t)
```

## Typescript
```ts
const s = $$js_src;
let t = 0;
for(let i=0;i<100000;i++) t += s.trim().length;
console.log(t);
```
