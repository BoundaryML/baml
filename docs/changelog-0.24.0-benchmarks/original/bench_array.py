import json, sys, time
import baml_sdk
fn = baml_sdk.array_traversal_bench_sync if sys.argv[1]=='024' else baml_sdk.array_traversal_bench
expected=fn()
start=time.perf_counter_ns()
for _ in range(20): assert fn()==expected
print(json.dumps({'ms_per_job':(time.perf_counter_ns()-start)/20/1e6,'result':expected}))
