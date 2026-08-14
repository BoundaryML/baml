# compute::divide guard 1m

## BAML
```baml
function maybe_divide(a: int, b: int) -> int {
  if b == 0 { 0 } else { a / b }
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += maybe_divide(i, i + 1); };
  return s;
}
```

## Python
```python
def maybe_divide(a, b):
    if b == 0: return 0
    return a // b
s = 0
for i in range(1000000): s += maybe_divide(i, i + 1)
print(s)
```

## Typescript
```ts
function md(a,b){return b===0?0:Math.trunc(a/b)}
let s=0;for(let i=0;i<1000000;i++)s+=md(i,i+1);console.log(s)
```
