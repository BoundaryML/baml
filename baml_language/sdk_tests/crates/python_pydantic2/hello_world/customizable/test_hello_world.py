from baml_sdk import hello_world


def test_hello_world() -> None:
    assert hello_world() == "hello world"
