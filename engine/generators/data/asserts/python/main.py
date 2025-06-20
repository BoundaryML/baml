import asyncio
from baml_client import b

async def main():
    result = await b.PersonTest()
    print(result)

    stream = b.stream.PersonTest()
    async for result in stream:
        print(result)
    done = await stream.get_final_response()
    print(done)

if __name__ == "__main__":
    asyncio.run(main())
