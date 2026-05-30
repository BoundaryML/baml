# string::split medium literal 10k

## eval-setup
```py
import json
chunk = "the quick brown fox jumps over the lazy dog and runs through the meadow"
src = "|".join([chunk] * 8)
delim = "|"
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
  for (let i = 0; i < 10000; i += 1) {
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
for _ in range(10000):
    c += len(s.split(delim))
print(c)
```

## Typescript
```ts
const s = $$js_src;
const delim = $$js_delim;
let c = 0;
for(let i=0;i<10000;i++) c += s.split(delim).length;
console.log(c);
```
