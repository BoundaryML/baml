# compute::generic apply with inferred type args 1m

Exercises inferred-generic-call frame seeding (a `LoadType` + wider `Call`
per iteration): `gapply<T>`'s `T` is inferred at every call site, so the caller
seeds `frame.type_args = [int]` each time. Guards the cost of `CallPlan.type_args`
threading against regressions; the rest of the compute suite is monomorphic and
cannot see it.

## BAML
```baml
function gapply<T>(f: (T) -> T, x: T) -> T { f(x) }
function main() -> int {
  let double = (x: int) -> int { x * 2 };
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += gapply(double, i); };
  return s;
}
```

## Python
```python
def gapply(f, x): return f(x)
double = lambda x: x * 2
s = 0
for i in range(1000000): s += gapply(double, i)
print(s)
```

## TypeScript
```ts
function gapply(f,x){return f(x)}
const double=x=>x*2;let s=0;
for(let i=0;i<1000000;i++)s+=gapply(double,i);console.log(s)
```
