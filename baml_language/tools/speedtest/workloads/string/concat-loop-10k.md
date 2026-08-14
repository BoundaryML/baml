# string::concat loop 10k

## BAML
```baml
function main() -> int {
  let s = "";
  for (let i = 0; i < 10000; i += 1) {
    s = s + "x";
  };
  return s.length();
}
```

## Python
```python
s = ""
for _ in range(10000):
    s = s + "x"
print(len(s))
```

## Typescript
```ts
let s="";for(let i=0;i<10000;i++) s = s + "x";
console.log(s.length);
```
