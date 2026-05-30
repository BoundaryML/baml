# compute::fib iterative 100k

## BAML
```baml
function fib(n: int) -> int {
  let a = 0;
  let b = 1;
  for (let i = 0; i < n; i += 1) {
    let tmp = b;
    b = a + b;
    a = tmp;
  };
  return a;
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 100000; i += 1) { s += fib(50); };
  return s;
}
```

## Python
```python
def fib(n):
    a, b = 0, 1
    for _ in range(n): a, b = b, a + b
    return a
s = 0
for i in range(100000): s += fib(50)
print(s)
```

## Typescript
```ts
function fib(n){let a=0,b=1;for(let i=0;i<n;i++){let t=b;b=a+b;a=t}return a}
let s=0;for(let i=0;i<100000;i++)s+=fib(50);console.log(s)
```
