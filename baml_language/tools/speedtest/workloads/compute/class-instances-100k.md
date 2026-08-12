# compute::class instances 100k

## BAML
```baml
class Point {
  x: int
  y: int
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 100000; i += 1) {
    let p = Point { x: i, y: i * 2 };
    s += p.x + p.y;
  };
  return s;
}
```

## Python
```python
class Point:
    __slots__ = ('x', 'y')
    def __init__(self, x, y): self.x = x; self.y = y
s = 0
for i in range(100000):
    p = Point(i, i * 2); s += p.x + p.y
print(s)
```

## Typescript
```ts
class Point{constructor(x,y){this.x=x;this.y=y}}
let s=0;for(let i=0;i<100000;i++){const p=new Point(i,i*2);s+=p.x+p.y}console.log(s)
```
