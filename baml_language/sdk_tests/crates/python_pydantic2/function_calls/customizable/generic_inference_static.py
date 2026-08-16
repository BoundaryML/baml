"""Static contract for inferred and explicitly-bound generic BAML calls."""

from typing import assert_type

from baml_sdk.generic_tests import (
    GenericBox,
    apply,
    identity,
    identity_async,
    identity_with_default,
    identity_with_default_async,
    one_type_arg,
    one_type_arg_async,
    optional_only,
    parse_as,
    parse_as_async,
    two_in_union,
    two_in_union_with_values,
)


def int_to_str(value: int) -> str:
    return str(value)


# Required value arguments infer own TypeVars for free sync/async functions,
# instance methods, and static methods. A later default stays keyword-only and
# does not make `_types=` required when every TypeVar is already inferable.
assert_type(identity(42), int)
assert_type(identity("inferred"), str)
assert_type(GenericBox[int](value=42).pair_with("inferred"), str)
assert_type(identity_with_default(42), int)
assert_type(identity_with_default("inferred", label="named"), str)
assert_type(
    GenericBox[int](value=42).pair_with_default("inferred", label="named"),
    str,
)
GenericBox.new(42)
GenericBox.new_with_default(42, label="named")
assert_type(two_in_union_with_values(1, "left", True), str)

# Defaulted value positions remain runtime inference sources even when omitted.
optional_only()
optional_only(x=7)
optional_only(_types={"T": int})

# Return/body-only TypeVars cannot be inferred from a value argument. The
# explicit `_types=` surface remains accepted for those calls.
parse_as("42", _types={"T": int})
assert_type(one_type_arg(_types={"T": int}), str)
assert_type(GenericBox.static_type_name(_types={"V": int}), str)
apply(int_to_str, 42, _types={"T": int, "R": str})

# These ignores are negative assertions: Pyright is configured to reject an
# unnecessary ignore, so each line proves the generated stub requires `_types`.
one_type_arg()  # pyright: ignore[reportCallIssue]
parse_as("42")  # pyright: ignore[reportCallIssue]
GenericBox.static_type_name()  # pyright: ignore[reportCallIssue]
apply(int_to_str, 42)  # pyright: ignore[reportCallIssue]
two_in_union(42)  # pyright: ignore[reportCallIssue]


async def inferred_async_calls() -> None:
    assert_type(await identity_async(42), int)
    assert_type(await identity_with_default_async(42, label="named"), int)
    await GenericBox.new_with_default_async(42, label="named")
    await GenericBox[int](value=42).pair_with_default_async("inferred")

    await parse_as_async("42", _types={"T": int})
    assert_type(await one_type_arg_async(_types={"T": int}), str)

    await parse_as_async("42")  # pyright: ignore[reportCallIssue]
    await one_type_arg_async()  # pyright: ignore[reportCallIssue]
