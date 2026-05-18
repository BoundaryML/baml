from __future__ import annotations

import pydantic

from baml_core import define_function as __define_function

# The BAML source lives in `baml_src/ns_lorem/root.baml`; the compiler
# strips the `ns_` prefix so its BAML FQN is `user.lorem.*` (engine
# convention, post-phase-12a). 09b §1 routes that to `baml_sdk.lorem.*`
# here.


class MyLorem(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    a: int


add_three_to_field_a = __define_function(
    "user.lorem.add_three_to_field_a", "sync", ["input_lorem"]
)
add_three_to_field_a_async = __define_function(
    "user.lorem.add_three_to_field_a", "async", ["input_lorem"]
)
default_score = __define_function(
    "user.lorem.default_score", "sync", ["query", "max_results", "filter"], 1
)
default_score_async = __define_function(
    "user.lorem.default_score", "async", ["query", "max_results", "filter"], 1
)
mutate_default = __define_function(
    "user.lorem.mutate_default", "sync", ["items"], 0
)
mutate_default_async = __define_function(
    "user.lorem.mutate_default", "async", ["items"], 0
)

__all__ = [
    "MyLorem",
    "add_three_to_field_a",
    "add_three_to_field_a_async",
    "default_score",
    "default_score_async",
    "mutate_default",
    "mutate_default_async",
]
