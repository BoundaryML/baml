# interfaces::default method 100k

## BAML
```baml
interface Counter {
  base: int
  function step(self) -> int
  function total(self) -> int {
    return self.base + self.step()
  }
}
class Inc {
  base: int
  implements Counter {
    function step(self) -> int { return 1 }
  }
}
class Dbl {
  base: int
  implements Counter {
    function step(self) -> int { return self.base }
  }
}
function pick(i: int) -> Counter {
  if i % 2 == 0 { return Inc { base: i }; };
  return Dbl { base: i };
}
function run(c: Counter) -> int {
  return c.total()
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += run(pick(i));
  };
  return acc;
}
```

## Python
```python
class Inc:
    __slots__ = ('base',)
    def __init__(self, base): self.base = base
    def step(self): return 1
    def total(self): return self.base + self.step()
class Dbl:
    __slots__ = ('base',)
    def __init__(self, base): self.base = base
    def step(self): return self.base
    def total(self): return self.base + self.step()
def pick(i):
    return Inc(i) if i % 2 == 0 else Dbl(i)
acc = 0
for i in range(100000):
    acc += pick(i).total()
print(acc)
```

## Typescript
```ts
class Inc{constructor(base){this.base=base}step(){return 1}total(){return this.base+this.step()}}
class Dbl{constructor(base){this.base=base}step(){return this.base}total(){return this.base+this.step()}}
function pick(i){return i%2===0?new Inc(i):new Dbl(i)}
let acc=0;for(let i=0;i<100000;i++){acc+=pick(i).total()}console.log(acc)
```
