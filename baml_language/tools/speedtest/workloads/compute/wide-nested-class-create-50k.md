# compute::wide nested class create 50k

## BAML
```baml
class Leaf {
  a: int
  b: int
  c: int
  d: int
}
class Node {
  left: Leaf
  right: Leaf
  n0: int
  n1: int
  n2: int
  n3: int
}
class BigRecord {
  id: int
  a0: int
  a1: int
  a2: int
  a3: int
  a4: int
  a5: int
  a6: int
  a7: int
  primary: Node
  secondary: Node
  tail: Leaf
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    let r = BigRecord {
      id: i,
      a0: i + 1,
      a1: i + 2,
      a2: i + 3,
      a3: i + 4,
      a4: i + 5,
      a5: i + 6,
      a6: i + 7,
      a7: i + 8,
      primary: Node {
        left: Leaf { a: i, b: i + 1, c: i + 2, d: i + 3 },
        right: Leaf { a: i + 4, b: i + 5, c: i + 6, d: i + 7 },
        n0: i + 8,
        n1: i + 9,
        n2: i + 10,
        n3: i + 11
      },
      secondary: Node {
        left: Leaf { a: i + 12, b: i + 13, c: i + 14, d: i + 15 },
        right: Leaf { a: i + 16, b: i + 17, c: i + 18, d: i + 19 },
        n0: i + 20,
        n1: i + 21,
        n2: i + 22,
        n3: i + 23
      },
      tail: Leaf { a: i + 24, b: i + 25, c: i + 26, d: i + 27 }
    };
    s += r.id + r.a7 + r.primary.left.c + r.primary.right.d + r.secondary.n3 + r.secondary.right.b + r.tail.d;
  };
  return s;
}
```

## Python
```python
class Leaf:
    __slots__ = ('a', 'b', 'c', 'd')
    def __init__(self, a, b, c, d): self.a = a; self.b = b; self.c = c; self.d = d
class Node:
    __slots__ = ('left', 'right', 'n0', 'n1', 'n2', 'n3')
    def __init__(self, left, right, n0, n1, n2, n3):
        self.left = left; self.right = right
        self.n0 = n0; self.n1 = n1; self.n2 = n2; self.n3 = n3
class BigRecord:
    __slots__ = ('id', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 'primary', 'secondary', 'tail')
    def __init__(self, id, a0, a1, a2, a3, a4, a5, a6, a7, primary, secondary, tail):
        self.id = id; self.a0 = a0; self.a1 = a1; self.a2 = a2; self.a3 = a3
        self.a4 = a4; self.a5 = a5; self.a6 = a6; self.a7 = a7
        self.primary = primary; self.secondary = secondary; self.tail = tail
s = 0
for i in range(50000):
    r = BigRecord(
        i, i + 1, i + 2, i + 3, i + 4, i + 5, i + 6, i + 7, i + 8,
        Node(Leaf(i, i + 1, i + 2, i + 3), Leaf(i + 4, i + 5, i + 6, i + 7), i + 8, i + 9, i + 10, i + 11),
        Node(Leaf(i + 12, i + 13, i + 14, i + 15), Leaf(i + 16, i + 17, i + 18, i + 19), i + 20, i + 21, i + 22, i + 23),
        Leaf(i + 24, i + 25, i + 26, i + 27),
    )
    s += r.id + r.a7 + r.primary.left.c + r.primary.right.d + r.secondary.n3 + r.secondary.right.b + r.tail.d
print(s)
```

## Typescript
```ts
class Leaf{constructor(a,b,c,d){this.a=a;this.b=b;this.c=c;this.d=d}}
class Node{constructor(left,right,n0,n1,n2,n3){this.left=left;this.right=right;this.n0=n0;this.n1=n1;this.n2=n2;this.n3=n3}}
class BigRecord{constructor(id,a0,a1,a2,a3,a4,a5,a6,a7,primary,secondary,tail){this.id=id;this.a0=a0;this.a1=a1;this.a2=a2;this.a3=a3;this.a4=a4;this.a5=a5;this.a6=a6;this.a7=a7;this.primary=primary;this.secondary=secondary;this.tail=tail}}
let s=0;
for(let i=0;i<50000;i++){
  const r=new BigRecord(i,i+1,i+2,i+3,i+4,i+5,i+6,i+7,i+8,
    new Node(new Leaf(i,i+1,i+2,i+3),new Leaf(i+4,i+5,i+6,i+7),i+8,i+9,i+10,i+11),
    new Node(new Leaf(i+12,i+13,i+14,i+15),new Leaf(i+16,i+17,i+18,i+19),i+20,i+21,i+22,i+23),
    new Leaf(i+24,i+25,i+26,i+27));
  s+=r.id+r.a7+r.primary.left.c+r.primary.right.d+r.secondary.n3+r.secondary.right.b+r.tail.d;
}
console.log(s);
```
