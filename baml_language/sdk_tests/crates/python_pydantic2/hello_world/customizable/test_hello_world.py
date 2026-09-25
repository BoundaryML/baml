from baml_sdk import hello_world


# SDK_PARITY_LINT(skip): checks the minimal generated function binding fixture
def test_hello_world():
    assert hello_world() == "hello world"
