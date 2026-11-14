import asyncio


async def nap():
    await asyncio.sleep(0.1)
    return 1


async def tree(depth):
    if depth == 0:
        return await nap()
    left = asyncio.ensure_future(tree(depth - 1))
    right = asyncio.ensure_future(tree(depth - 1))
    return await left + await right + 1


async def main():
    return await tree(13)
