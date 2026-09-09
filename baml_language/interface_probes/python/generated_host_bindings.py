from typing_extensions import assert_type
from baml_sdk import GreeterHost, GreeterRef, welcome_async


class Greeting:
    async def greet(self, name: str) -> str:
        return name


class SyncGreeting:
    def greet(self, name: str) -> str:
        return name


async def check() -> None:
    asynchronous: GreeterHost = Greeting()
    synchronous: GreeterHost = SyncGreeting()
    first = await GreeterRef.bind(asynchronous)
    second = await GreeterRef.bind(synchronous)
    assert_type(first, GreeterRef)
    assert_type(await first.greet("Ada"), str)
    assert_type(await second.label(), str)
    assert_type(await welcome_async(second, "Ada"), str)

