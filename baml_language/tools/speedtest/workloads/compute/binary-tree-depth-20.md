# compute::binary tree depth 20

## BAML
```baml
function tree_sum(depth: int) -> int {
  if depth <= 0 { 1 } else { tree_sum(depth - 1) + tree_sum(depth - 1) }
}
function main() -> int { tree_sum(20) }
```

## Python
```python
def t(d):
    if d <= 0: return 1
    return t(d-1) + t(d-1)
print(t(20))
```

## Typescript
```ts
function t(d){return d<=0?1:t(d-1)+t(d-1)}console.log(t(20))
```
