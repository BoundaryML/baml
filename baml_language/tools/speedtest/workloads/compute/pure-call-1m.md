# compute::pure call 1m

T27 (BEX tracing review): the call-heavy workload that exposes *per-call*
overhead — 1M trivial depth-1 calls, so the function-call machinery (frame
push/pop, and with `BAML_PROFILE=1` the CallFunction/EndFunction ring pair)
dominates the runtime. The depth-100 `call-chain-100x10k` workload amortizes
per-call cost across deep stacks; this one does not — it is the speedtest
twin of the pure-call microbench that measured ~63 ns/pair.

## eval-setup
```py
baml = """
function leaf(n: int) -> int { n }

function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += leaf(i); };
  return s;
}"""

python = """
def leaf(n):
    return n

s = 0
for i in range(1000000):
    s += leaf(i)
print(s)
"""

js = """
function leaf(n){return n}
let s=0;for(let i=0;i<1000000;i++)s+=leaf(i);console.log(s);
"""
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
