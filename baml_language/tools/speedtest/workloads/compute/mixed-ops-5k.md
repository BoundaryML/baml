# compute::mixed ops 5k

## BAML
```baml
class Point {
  x: int
  y: int
}
function main() -> int {
  let sum = 0;
  let s = "";
  let arr: int[] = [];
  for (let i = 0; i < 5000; i += 1) {
    sum += i * 3 - 1;
    s = s + "x";
    arr.push(i);
    let p = Point { x: i, y: i + 1 };
    sum += p.x + p.y;
    if i > 2500 { sum += 1; };
  };
  return sum + s.length() + arr.length();
}
```

## Python
```python
class Point:
    __slots__ = ('x', 'y')
    def __init__(self, x, y): self.x = x; self.y = y
sum = 0
s = ""
arr = []
for i in range(5000):
    sum += i * 3 - 1
    s = s + "x"
    arr.append(i)
    p = Point(i, i + 1)
    sum += p.x + p.y
    if i > 2500: sum += 1
print(sum + len(s) + len(arr))
```

## Typescript
```ts
class Point{constructor(x,y){this.x=x;this.y=y}}
let sum=0;let s="";const arr=[];
for(let i=0;i<5000;i++){
  sum+=i*3-1;
  s=s+"x";
  arr.push(i);
  const p=new Point(i,i+1);
  sum+=p.x+p.y;
  if(i>2500)sum+=1;
}
console.log(sum+s.length+arr.length);
```
