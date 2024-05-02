import baml_py
import random
import asyncio

async def fetch_data(url: str):
    print(f"Fetching data from {url}...")
    await asyncio.sleep(1)
    print(f"Received data from {url}!")
    return f"Data from {url}"

async def rust_sleep():
    i = random.randint(1000, 10000)
    print(f"{i}: going to sleep in rust")
    await baml_py.rust_sleep()
    print(f"{i}: woke up from rust")

async def main():
    b = baml_py.BamlRuntimeFfi.from_directory("/home/sam/baml/integ-tests/baml_src")

    tasks = [
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Bayes"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Higgs"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Boson"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Bayes"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Higgs"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Boson"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Bayes"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Higgs"}, ctx = {}),
        b.call_async("ExtractNames", {"input": "We're generally a group of folks like Boson"}, ctx = {}),
    ]
    for t in tasks:
        print(repr(t))
    results = await asyncio.gather(*tasks)
    for result in results:
        print(result)

import time

start_time = time.perf_counter()

asyncio.run(main())

end_time = time.perf_counter()
elapsed_time = end_time - start_time
print(f"Elapsed time: {elapsed_time:.2f} seconds")
