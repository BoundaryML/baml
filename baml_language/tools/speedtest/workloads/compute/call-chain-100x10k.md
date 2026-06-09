# compute::call chain 100x10k

## eval-setup
```py
baml_lines = []
for i in range(99):
    baml_lines.append(f"function f{i}(n: int) -> int {{ f{i+1}(n + 1) }}")
baml_lines.append("function f99(n: int) -> int { n }")
baml_lines.append("""
function main() -> int {
  let s = 0;
  for (let i = 0; i < 10000; i += 1) { s += f0(0); };
  return s;
}""")
baml = "\n".join(baml_lines)

py_lines = []
for i in range(99):
    py_lines.append(f"def f{i}(n): return f{i+1}(n + 1)")
py_lines.append("def f99(n): return n")
py_lines.append("s = 0")
py_lines.append("for _ in range(10000): s += f0(0)")
py_lines.append("print(s)")
python = "\n".join(py_lines)

js_lines = []
for i in range(99):
    js_lines.append(f"function f{i}(n){{return f{i+1}(n+1)}}")
js_lines.append("function f99(n){return n}")
js_lines.append("let s=0;for(let i=0;i<10000;i++)s+=f0(0);console.log(s);")
js = "\n".join(js_lines)
```

## BAML
```baml
$$baml
```

## Python
```python
$$python
```

## Typescript
```ts
$$js
```
