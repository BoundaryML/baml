# compute::fib32 recursive

## BAML
```baml
function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(32) }
```

## Python
```python
def fib(n):
    if n <= 1: return n
    return fib(n - 1) + fib(n - 2)
print(fib(32))
```

## Typescript
```ts
function fib(n){return n<=1?n:fib(n-1)+fib(n-2)}console.log(fib(32))
```
