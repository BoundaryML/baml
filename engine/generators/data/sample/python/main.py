import json
import asyncio
from baml_client import b

async def main():
    # result = b.foo(8192)
    # print(result)

    stream = b.stream.Foo(8192)
    async for result in stream:
        print(result)
    done = await stream.get_final_response()
    print(done)

if __name__ == "__main__":
    asyncio.run(main())

