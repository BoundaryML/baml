# compute::array build sum 100k

## BAML
```baml
function main() -> int {
  let arr: int[] = [];
  for (let i = 0; i < 100000; i += 1) { arr.push(i); };
  let s = 0;
  for (let x in arr) { s += x; };
  return s;
}
```

## Python
```python
arr = []
for i in range(100000): arr.append(i)
s = 0
for x in arr: s += x
print(s)
```

## Typescript
```ts
const a=[];for(let i=0;i<100000;i++)a.push(i);
let s=0;for(const x of a)s+=x;console.log(s)
```
