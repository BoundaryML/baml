# interfaces::match dispatch 100k

## BAML
```baml
interface Animal {
  function noise(self) -> int
}
class Dog {
  implements Animal {
    function noise(self) -> int { return 3 }
  }
}
class Cat {
  implements Animal {
    function noise(self) -> int { return 5 }
  }
}
function pick(i: int) -> Animal {
  if i % 2 == 0 { return Dog {}; };
  return Cat {};
}
function code(a: Animal) -> int {
  return match (a) {
    let d: Dog => d.noise()
    let c: Cat => c.noise()
    _ => 0
  }
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += code(pick(i));
  };
  return acc;
}
```

## Python
```python
class Dog:
    __slots__ = ()
    def noise(self): return 3
class Cat:
    __slots__ = ()
    def noise(self): return 5
def pick(i):
    return Dog() if i % 2 == 0 else Cat()
def code(a):
    if isinstance(a, Dog): return a.noise()
    elif isinstance(a, Cat): return a.noise()
    else: return 0
acc = 0
for i in range(100000):
    acc += code(pick(i))
print(acc)
```

## Typescript
```ts
class Dog{noise(){return 3}}
class Cat{noise(){return 5}}
function pick(i){return i%2===0?new Dog():new Cat()}
function code(a){if(a instanceof Dog){return a.noise()}else if(a instanceof Cat){return a.noise()}else{return 0}}
let acc=0;for(let i=0;i<100000;i++){acc+=code(pick(i))}console.log(acc)
```
