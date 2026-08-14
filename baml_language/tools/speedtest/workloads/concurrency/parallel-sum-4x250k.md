# concurrency::parallel sum 4x250k

## BAML
```baml
function sum_range(start: int, end: int) -> int {
  let s = 0;
  let i = start;
  while i < end {
    s += i;
    i += 1;
  };
  s
}
function main() -> int {
  let a = spawn { sum_range(0, 250000) };
  let b = spawn { sum_range(250000, 500000) };
  let c = spawn { sum_range(500000, 750000) };
  let d = spawn { sum_range(750000, 1000000) };
  (await a) + (await b) + (await c) + (await d)
}
```

## Python
```python
from concurrent.futures import ThreadPoolExecutor
def sum_range(start, end):
    s = 0
    for i in range(start, end): s += i
    return s
with ThreadPoolExecutor(max_workers=4) as ex:
    fs = [ex.submit(sum_range, s, e) for s, e in [(0,250000),(250000,500000),(500000,750000),(750000,1000000)]]
    print(sum(f.result() for f in fs))
```

## Typescript
```ts
async function sumRange(start, end){
  let s = 0;
  for(let i=start;i<end;i++) s += i;
  return s;
}
(async()=>{
  const [a,b,c,d] = await Promise.all([
    sumRange(0,250000),
    sumRange(250000,500000),
    sumRange(500000,750000),
    sumRange(750000,1000000),
  ]);
  console.log(a+b+c+d);
})();
```
