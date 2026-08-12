# compute::nested field access 500k

## BAML
```baml
class Inner { value: int }
class Middle { inner: Inner }
class Outer { middle: Middle }
function main() -> int {
  let s = 0;
  for (let i = 0; i < 500000; i += 1) {
    let o = Outer { middle: Middle { inner: Inner { value: i } } };
    s += o.middle.inner.value;
  };
  return s;
}
```

## Python
```python
class Inner:
    __slots__ = ('value',)
    def __init__(self, v): self.value = v
class Middle:
    __slots__ = ('inner',)
    def __init__(self, i): self.inner = i
class Outer:
    __slots__ = ('middle',)
    def __init__(self, m): self.middle = m
s = 0
for i in range(500000):
    o = Outer(Middle(Inner(i)))
    s += o.middle.inner.value
print(s)
```

## Typescript
```ts
class Inner{constructor(v){this.value=v}}
class Middle{constructor(i){this.inner=i}}
class Outer{constructor(m){this.middle=m}}
let s=0;for(let i=0;i<500000;i++){const o=new Outer(new Middle(new Inner(i)));s+=o.middle.inner.value}console.log(s)
```
