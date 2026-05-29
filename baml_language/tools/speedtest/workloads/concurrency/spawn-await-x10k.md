# concurrency::spawn await x10k

## BAML
```baml
function main() -> int {
  let s = 0;
  for (let i = 0; i < 10000; i += 1) {
    s += await spawn { 1 };
  };
  s
}
```

## Python
```python
import threading
s = 0
def f(out, idx):
    out[idx] = 1
out = [0] * 10000
for i in range(10000):
    t = threading.Thread(target=f, args=(out, i))
    t.start(); t.join()
    s += out[i]
print(s)
```

## Typescript
```ts
async function main(){
  let s=0;
  for(let i=0;i<10000;i++) s += await Promise.resolve(1);
  console.log(s);
}
main();
```
