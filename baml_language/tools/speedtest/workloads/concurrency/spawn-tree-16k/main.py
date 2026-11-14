import asyncio


async def tree(depth):
    if depth == 0:
        return 1
    left = asyncio.ensure_future(tree(depth - 1))
    right = asyncio.ensure_future(tree(depth - 1))
    return await left + await right + 1


async def main():
    return await tree(13)
