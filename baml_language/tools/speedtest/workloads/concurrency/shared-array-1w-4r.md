# concurrency::shared array 1w 4r

## BAML
```baml
function write_n(arr: int[], n: int) -> int {
  let i = 0;
  while i < n { arr.push(i); i += 1; };
  0
}
function read_n(arr: int[], n: int) -> int {
  let i = 0;
  let acc = 0;
  while i < n {
    let len = arr.length();
    if len > 0 { acc = acc + arr[0]; };
    i += 1;
  };
  acc
}
function main() -> int {
  let arr = [0];
  let w = spawn { write_n(arr, 50000) };
  let r1 = spawn { read_n(arr, 12500) };
  let r2 = spawn { read_n(arr, 12500) };
  let r3 = spawn { read_n(arr, 12500) };
  let r4 = spawn { read_n(arr, 12500) };
  (await w) + (await r1) + (await r2) + (await r3) + (await r4);
  arr.length()
}
```

## Python
```python
import threading
def write_n(arr, n):
    for i in range(n): arr.append(i)
def read_n(arr, n, out, idx):
    acc = 0
    for _ in range(n):
        if len(arr) > 0: acc += arr[0]
    out[idx] = acc
arr = [0]
out = [0]*4
w = threading.Thread(target=write_n, args=(arr, 50000))
rs = [threading.Thread(target=read_n, args=(arr, 12500, out, i)) for i in range(4)]
w.start()
for r in rs: r.start()
w.join()
for r in rs: r.join()
print(len(arr))
```

## Typescript
```ts
function writeN(arr,n){ for(let i=0;i<n;i++) arr.push(i); }
function readN(arr,n){
  let acc=0;
  for(let i=0;i<n;i++){
    if(arr.length>0) acc += arr[0];
  }
  return acc;
}
const arr = [0];
writeN(arr, 50000);
readN(arr, 12500); readN(arr, 12500); readN(arr, 12500); readN(arr, 12500);
console.log(arr.length);
```
