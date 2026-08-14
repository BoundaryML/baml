# string::string split 100k

## BAML
```baml
function main() -> int {
  let count = 0;
  for (let i = 0; i < 100000; i += 1) {
    let s = "hello world foo bar baz qux";
    let parts = s.split(" ");
    count += parts.length();
  };
  return count;
}
```

## Python
```python
c = 0
for i in range(100000):
    s = "hello world foo bar baz qux"
    c += len(s.split(" "))
print(c)
```

## Typescript
```ts
let c=0;for(let i=0;i<100000;i++){c+="hello world foo bar baz qux".split(" ").length}console.log(c)
```
