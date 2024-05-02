import asyncio
import baml_py
import time

from baml_client import client

b = baml_py.BamlRuntimeFfi.from_directory("/home/sam/baml/integ-tests/baml_src")
b2 = client.BamlClient.from_directory("/home/sam/baml/integ-tests/baml_src")



async def timeit(b):
    start_time = time.perf_counter()
    await b.call_function("ExtractNames", {"input": "We're generally a group of folks like Bayes"}, ctx = {})
    end_time = time.perf_counter()
    return end_time - start_time


async def main():

    tasks = [ b2.V2_UnionTest_Function(input="asdf") for _ in range(10)]
    retvals = await asyncio.gather(*tasks)
    print("retvals: ", retvals)


    for _ in range(10):
        print("Kicking off N tasks")
        start_time = time.perf_counter()

        tasks = [ timeit(b) for _ in range(10) ]
        timings = await asyncio.gather(*tasks)

        end_time = time.perf_counter()
        elapsed_time = end_time - start_time
        print(f"Elapsed time: {elapsed_time:.2f} seconds")
        print("Results: {}".format([f"{t:.3f}s" for t in timings]))

start_time = time.perf_counter()

asyncio.run(main())

end_time = time.perf_counter()
elapsed_time = end_time - start_time
print(f"Elapsed time: {elapsed_time:.2f} seconds")
