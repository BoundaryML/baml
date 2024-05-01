import baml_py
import asyncio

async def fetch_data(url):
    print(f"Fetching data from {url}...")
    await asyncio.sleep(2)  # simulate a 2-second delay
    print(f"Received data from {url}!")
    return f"Data from {url}"

async def baml_extract_names():
    b = baml_py.BamlRuntimeFfi.from_directory("/home/sam/baml/integ-tests/baml_src")
    res = b.call_function("ExtractNames", {"input": "We're generally a group of folks like Bayes"}, ctx = {})
    print(res)
    return res

async def main():

    tasks = [
        fetch_data("https://example.com/data1"),
        fetch_data("https://example.com/data2"),
        fetch_data("https://example.com/data3")
        baml_extract_names()
    ]

    results = await asyncio.gather(*tasks)
    for result in results:
        print(result)

asyncio.run(main())
