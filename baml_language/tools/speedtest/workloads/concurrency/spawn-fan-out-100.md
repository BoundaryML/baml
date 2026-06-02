# concurrency::spawn fan out 100

## eval-setup
```py
lines = ["function task(i: int) -> int { i * 2 }", "function main() -> int {"]
for i in range(100):
    lines.append(f"  let f{i} = spawn {{ task({i}) }};")
expr = " + ".join([f"(await f{i})" for i in range(100)])
lines.append(f"  {expr}")
lines.append("}")
baml = "\n".join(lines)
```

## BAML
```baml
$$baml
```

## Python
```python
from concurrent.futures import ThreadPoolExecutor
def task(i): return i * 2
with ThreadPoolExecutor(max_workers=4) as ex:
    futures = [ex.submit(task, i) for i in range(100)]
    print(sum(f.result() for f in futures))
```

## Typescript
```ts
async function task(i){ return i * 2; }
(async()=>{
  const fs = [];
  for(let i=0;i<100;i++) fs.push(task(i));
  let s=0;
  for(const f of fs) s += await f;
  console.log(s);
})();
```
