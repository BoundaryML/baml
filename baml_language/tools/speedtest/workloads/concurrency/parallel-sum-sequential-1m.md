# concurrency::parallel sum sequential 1m

## BAML
```baml
function main() -> int {
  let s = 0;
  let i = 0;
  while i < 1000000 {
    s += i;
    i += 1;
  };
  s
}
```

## Python
```python
s = 0
for i in range(1000000): s += i
print(s)
```

## Typescript
```ts
let s = 0;
for(let i=0;i<1000000;i++) s += i;
console.log(s);
```
