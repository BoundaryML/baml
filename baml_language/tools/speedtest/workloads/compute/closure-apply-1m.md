# compute::closure apply 1m

## BAML
```baml
function apply(f: (int) -> int, x: int) -> int { f(x) }
function main() -> int {
  let double = (x: int) -> int { x * 2 };
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += apply(double, i); };
  return s;
}
```

## Python
```python
def apply(f, x): return f(x)
double = lambda x: x * 2
s = 0
for i in range(1000000): s += apply(double, i)
print(s)
```

## Typescript
```ts
function apply(f,x){return f(x)}
const double=x=>x*2;let s=0;
for(let i=0;i<1000000;i++)s+=apply(double,i);console.log(s)
```
