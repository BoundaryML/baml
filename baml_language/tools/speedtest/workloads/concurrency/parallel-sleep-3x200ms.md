# concurrency::parallel sleep 3x200ms

## BAML
```baml
function nap() -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(200n)) catch (e) { let e => 0 };
  1
}
function main() -> int {
  let a = spawn { nap() };
  let b = spawn { nap() };
  let c = spawn { nap() };
  (await a) + (await b) + (await c)
}
```

## Python
```python
import asyncio
async def nap():
    await asyncio.sleep(0.2)
    return 1
async def main():
    a,b,c = await asyncio.gather(nap(), nap(), nap())
    print(a+b+c)
asyncio.run(main())
```

## Typescript
```ts
async function nap(){
  await new Promise(r => setTimeout(r, 200));
  return 1;
}
(async()=>{
  const [a,b,c] = await Promise.all([nap(), nap(), nap()]);
  console.log(a+b+c);
})();
```
