# concurrency::shared array push 4x25k

## BAML
```baml
function push_n(arr: int[], start: int, n: int) -> int {
  let i = 0;
  while i < n {
    arr.push(start + i);
    i += 1;
  };
  0
}
function main() -> int {
  let arr = [];
  let a = spawn { push_n(arr, 0, 25000) };
  let b = spawn { push_n(arr, 25000, 25000) };
  let c = spawn { push_n(arr, 50000, 25000) };
  let d = spawn { push_n(arr, 75000, 25000) };
  (await a) + (await b) + (await c) + (await d);
  arr.length()
}
```

## Python
```python
import threading
def push_n(arr, start, n):
    for i in range(n): arr.append(start + i)
arr = []
ts = [threading.Thread(target=push_n, args=(arr, i*25000, 25000)) for i in range(4)]
for t in ts: t.start()
for t in ts: t.join()
print(len(arr))
```

## Typescript
```ts
function pushN(arr, start, n){
  for(let i=0;i<n;i++) arr.push(start + i);
}
const arr = [];
pushN(arr, 0, 25000);
pushN(arr, 25000, 25000);
pushN(arr, 50000, 25000);
pushN(arr, 75000, 25000);
console.log(arr.length);
```
