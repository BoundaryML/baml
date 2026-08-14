# interfaces::polymorphic dispatch 100k

## BAML
```baml
interface Shape {
  function area(self) -> int
}
class Square {
  side: int
  implements Shape {
    function area(self) -> int { return self.side * self.side }
  }
}
class Rect {
  w: int
  h: int
  implements Shape {
    function area(self) -> int { return self.w * self.h }
  }
}
function pick(i: int) -> Shape {
  if i % 2 == 0 { return Square { side: i }; };
  return Rect { w: i, h: 3 };
}
function area_of(s: Shape) -> int {
  return s.area()
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += area_of(pick(i));
  };
  return acc;
}
```

## Python
```python
class Square:
    __slots__ = ('side',)
    def __init__(self, side): self.side = side
    def area(self): return self.side * self.side
class Rect:
    __slots__ = ('w', 'h')
    def __init__(self, w, h): self.w = w; self.h = h
    def area(self): return self.w * self.h
def pick(i):
    return Square(i) if i % 2 == 0 else Rect(i, 3)
acc = 0
for i in range(100000):
    acc += pick(i).area()
print(acc)
```

## Typescript
```ts
class Square{constructor(side){this.side=side}area(){return this.side*this.side}}
class Rect{constructor(w,h){this.w=w;this.h=h}area(){return this.w*this.h}}
function pick(i){return i%2===0?new Square(i):new Rect(i,3)}
let acc=0;for(let i=0;i<100000;i++){acc+=pick(i).area()}console.log(acc)
```
