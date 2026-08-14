# compute::nested loops 500x500

## BAML
```baml
function main() -> int {
  let s = 0;
  for (let i = 0; i < 500; i += 1) {
    for (let j = 0; j < 500; j += 1) { s += i * j; };
  };
  return s;
}
```

## Python
```python
s = 0
for i in range(500):
    for j in range(500):
        s += i * j
print(s)
```

## Typescript
```ts
let s=0;for(let i=0;i<500;i++)for(let j=0;j<500;j++)s+=i*j;console.log(s)
```
