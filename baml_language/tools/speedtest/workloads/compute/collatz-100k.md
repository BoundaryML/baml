# compute::collatz 100k

## BAML
```baml
function collatz_steps(n: int) -> int {
  let steps = 0;
  let x = n;
  while x != 1 {
    if x % 2 == 0 { x = x / 2; }
    else { x = 3 * x + 1; };
    steps += 1;
  };
  return steps;
}
function main() -> int {
  let s = 0;
  for (let i = 1; i < 100000; i += 1) { s += collatz_steps(i); };
  return s;
}
```

## Python
```python
def collatz_steps(n):
    steps = 0
    x = n
    while x != 1:
        if x % 2 == 0: x = x // 2
        else: x = 3 * x + 1
        steps += 1
    return steps
s = 0
for i in range(1, 100000): s += collatz_steps(i)
print(s)
```

## Typescript
```ts
function collatz(n){let s=0,x=n;while(x!==1){x=x%2===0?x/2:3*x+1;s++}return s}
let s=0;for(let i=1;i<100000;i++)s+=collatz(i);console.log(s)
```
