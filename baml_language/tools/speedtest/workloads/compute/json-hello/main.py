import json


class Hello:
    __slots__ = ("message",)

    def __init__(self, message):
        self.message = message


def setup():
    return Hello("Hello, World!")


def run(hello):
    return json.dumps({"message": hello.message}, separators=(",", ":"))


def main():
    return run(setup())
