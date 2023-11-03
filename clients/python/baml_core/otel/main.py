from .tracer import trace
from .provider import set_tags

# use_tracing()


@trace
def parent_function(a: int) -> int:
    """
    This is a parent function
    """
    set_tags(a="bar", b="car")
    child_function(a)
    return a + 10


@trace
def child_function(a: int) -> None:
    set_tags(c="dar", d="ear")
    set_tags(a="far")
    if a > 5:
        child_function(a - 1)
    child_foo(a)


@trace
def child_foo(a: int) -> None:
    print("hiii", a)


if __name__ == "__main__":
    parent_function(6)
    parent_function(8)
    # import concurrent.futures

    # with concurrent.futures.ThreadPoolExecutor() as executor:
    #     for _ in range(100):
    #         executor.submit(parent_function, 10)
