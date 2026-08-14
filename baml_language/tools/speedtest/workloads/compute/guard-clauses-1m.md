# compute::guard clauses 1m

## BAML
```baml
function validate(x: int) -> int {
  if x < 0 { return -1; };
  if x > 1000000 { return -2; };
  if x % 7 == 0 { return x * 2; };
  if x % 3 == 0 { return x + 1; };
  return x;
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += validate(i); };
  return s;
}
```

## Python
```python
def validate(x):
    if x < 0: return -1
    if x > 1000000: return -2
    if x % 7 == 0: return x * 2
    if x % 3 == 0: return x + 1
    return x
s = 0
for i in range(1000000): s += validate(i)
print(s)
```

## Typescript
```ts
function validate(x){if(x<0)return -1;if(x>1000000)return -2;if(x%7===0)return x*2;if(x%3===0)return x+1;return x}
let s=0;for(let i=0;i<1000000;i++)s+=validate(i);console.log(s)
```
