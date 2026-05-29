# compute::grid alloc 1000x100

## BAML
```baml
function main() -> int {
  let grid: int[][] = [];
  for (let i = 0; i < 1000; i += 1) {
    let row: int[] = [];
    for (let j = 0; j < 100; j += 1) { row.push(i * 100 + j); };
    grid.push(row);
  };
  let s = 0;
  for (let row in grid) {
    for (let val in row) { s += val; };
  };
  return s;
}
```

## Python
```python
grid = []
for i in range(1000):
    row = []
    for j in range(100): row.append(i * 100 + j)
    grid.append(row)
s = 0
for row in grid:
    for val in row: s += val
print(s)
```

## Typescript
```ts
const grid=[];for(let i=0;i<1000;i++){const row=[];for(let j=0;j<100;j++)row.push(i*100+j);grid.push(row)}
let s=0;for(const row of grid)for(const v of row)s+=v;console.log(s)
```
