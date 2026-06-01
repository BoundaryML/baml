# compute::if else dispatch 1m

## BAML
```baml
function classify(x: int) -> int {
  if x % 5 == 0 { 1 }
  else if x % 5 == 1 { 2 }
  else if x % 5 == 2 { 3 }
  else if x % 5 == 3 { 4 }
  else { 5 }
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += classify(i); };
  return s;
}
```

## Python
```python
def classify(x):
    r = x % 5
    if r == 0: return 1
    elif r == 1: return 2
    elif r == 2: return 3
    elif r == 3: return 4
    else: return 5
s = 0
for i in range(1000000): s += classify(i)
print(s)
```

## Typescript
```ts
function classify(x){const r=x%5;if(r===0)return 1;if(r===1)return 2;if(r===2)return 3;if(r===3)return 4;return 5}
let s=0;for(let i=0;i<1000000;i++)s+=classify(i);console.log(s)
```
