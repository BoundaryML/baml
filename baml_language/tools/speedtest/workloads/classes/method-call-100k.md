# classes::method call 100k

## BAML
```baml
class Vec {
  x: int
  y: int
  function norm2(self) -> int {
    return self.x * self.x + self.y * self.y
  }
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    let v = Vec { x: i, y: i + 1 };
    acc += v.norm2();
  };
  return acc;
}
```

## Python
```python
class Vec:
    __slots__ = ('x', 'y')
    def __init__(self, x, y): self.x = x; self.y = y
    def norm2(self): return self.x * self.x + self.y * self.y
acc = 0
for i in range(100000):
    v = Vec(i, i + 1); acc += v.norm2()
print(acc)
```

## Typescript
```ts
class Vec{constructor(x,y){this.x=x;this.y=y}norm2(){return this.x*this.x+this.y*this.y}}
let acc=0;for(let i=0;i<100000;i++){const v=new Vec(i,i+1);acc+=v.norm2()}console.log(acc)
```
