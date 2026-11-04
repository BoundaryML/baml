"""Static contract for inferred and explicitly-bound generic BAML calls."""

from typing_extensions import assert_type

from baml_bridge import BamlType
from baml_sdk import reflect
from baml_sdk.generic_tests import (
    GenericBox,
    apply_sync,
    identity_sync,
    identity,
    identity_with_default_sync,
    identity_with_default,
    one_type_arg_sync,
    one_type_arg,
    optional_only_sync,
    parse_as_sync,
    parse_as,
    two_in_union_sync,
    two_in_union_with_values_sync,
)


def int_to_str(value: int) -> str:
    return str(value)


# Required value arguments infer own TypeVars for free sync/async functions,
# instance methods, and static methods. A later default stays keyword-only and
# does not make `_types=` required when every TypeVar is already inferable.
assert_type(identity_sync(42), int)
assert_type(identity_sync("inferred"), str)
assert_type(GenericBox[int](value=42).pair_with_sync("inferred"), str)
assert_type(identity_with_default_sync(42), int)
assert_type(identity_with_default_sync("inferred", label="named"), str)
assert_type(
    GenericBox[int](value=42).pair_with_default_sync("inferred", label="named"),
    str,
)
GenericBox.new_sync(42)
GenericBox.new_with_default_sync(42, label="named")
assert_type(two_in_union_with_values_sync(1, "left", True), str)

# Defaulted value positions remain runtime inference sources even when omitted.
optional_only_sync()
optional_only_sync(x=7)
optional_only_sync(_types={"T": int})

# Reflected BAML type handles are also valid type tokens. Cover both generated
# stub shapes: inferable generics make `_types` optional, while body-only
# generics require it.
reflected_int: BamlType = reflect.Type.of(int)
optional_only_sync(_types={"T": reflected_int})
assert_type(one_type_arg_sync(_types={"T": reflected_int}), str)

# Return/body-only TypeVars cannot be inferred from a value argument. The
# explicit `_types=` surface remains accepted for those calls.
parse_as_sync("42", _types={"T": int})
assert_type(one_type_arg_sync(_types={"T": int}), str)
assert_type(GenericBox.static_type_name_sync(_types={"V": int}), str)
apply_sync(int_to_str, 42, _types={"T": int, "R": str})

# These ignores are negative assertions: Pyright is configured to reject an
# unnecessary ignore, so each line proves the generated stub requires `_types`.
one_type_arg_sync()  # pyright: ignore[reportCallIssue]
parse_as_sync("42")  # pyright: ignore[reportCallIssue]
GenericBox.static_type_name_sync()  # pyright: ignore[reportCallIssue]
apply_sync(int_to_str, 42)  # pyright: ignore[reportCallIssue]
two_in_union_sync(42)  # pyright: ignore[reportCallIssue]


async def inferred_async_calls() -> None:
    assert_type(await identity(42), int)
    assert_type(await identity_with_default(42, label="named"), int)
    await GenericBox.new_with_default(42, label="named")
    await GenericBox[int](value=42).pair_with_default("inferred")

    await parse_as("42", _types={"T": int})
    assert_type(await one_type_arg(_types={"T": int}), str)

    await parse_as("42")  # pyright: ignore[reportCallIssue]
    await one_type_arg()  # pyright: ignore[reportCallIssue]
