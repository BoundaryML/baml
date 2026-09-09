from baml_sdk import GreeterRef, welcome_async


class WrongReturn:
    def greet(self, name: str) -> int:
        return len(name)


class KeywordOnly:
    def greet(self, *, name: str) -> str:
        return name


class Greeting:
    def greet(self, name: str) -> str:
        return name


async def check() -> None:
    await GreeterRef.bind(WrongReturn())  # expected-error
    await GreeterRef.bind(KeywordOnly())  # expected-error
    await GreeterRef.bind(object())  # expected-error
    await welcome_async(Greeting(), "Ada")  # expected-error
