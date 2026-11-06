import asyncio, gc, json, sys, time
import baml_sdk as sdk
new = sys.argv[1] == '024'
def sync(name): return getattr(sdk, name + ('_sync' if new else ''))
def asynchronous(name): return getattr(sdk, name + ('' if new else '_async'))
results = {}
for name in ['template_bench', 'join_bench', 'enum_bench', 'nbody_bench']:
    fn = sync(name)
    warmup = fn()
    start = time.perf_counter_ns()
    actual = fn()
    elapsed = (time.perf_counter_ns() - start) / 1e6
    assert actual == warmup
    results[name] = {'ms': elapsed, 'result': actual}
fn=sync('noop')
for _ in range(100): assert fn()==1
start=time.perf_counter_ns()
for _ in range(5000): assert fn()==1
results['python_sync_noop']={'us_per_call': (time.perf_counter_ns()-start)/5000/1000}
async def run():
    fn=asynchronous('noop')
    for _ in range(100): assert await fn()==1
    start=time.perf_counter_ns()
    for _ in range(5000): assert await fn()==1
    results['python_async_sequential_noop']={'us_per_call':(time.perf_counter_ns()-start)/5000/1000}
    start=time.perf_counter_ns()
    out=await asyncio.gather(*(fn() for _ in range(5000)))
    assert out==[1]*5000
    results['python_async_gather_noop']={'us_per_call':(time.perf_counter_ns()-start)/5000/1000}
asyncio.run(run())
print(json.dumps(results))
