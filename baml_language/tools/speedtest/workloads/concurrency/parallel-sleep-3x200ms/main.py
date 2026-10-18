import asyncio

async def nap():
    await asyncio.sleep(0.2)
    return 1

async def main():
    a, b, c = await asyncio.gather(nap(), nap(), nap())
    return a + b + c
