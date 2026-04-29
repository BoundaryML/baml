from __future__ import annotations

import pydantic

from baml.baml_core import define_function as __define_function

# The BAML source lives in `baml_src/ns_lorem/root.baml`; the compiler
# strips the `ns_` prefix so its BAML FQN is `root.lorem.*`. 09b §1 routes
# that to `baml_sdk.lorem.*` here. The bridge translates `root.` → `user.`
# before hitting the engine.


class MyLorem(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    a: int


add_three_to_field_a = __define_function(
    "root.lorem.add_three_to_field_a", "sync", ["input_lorem"]
)
add_three_to_field_a_async = __define_function(
    "root.lorem.add_three_to_field_a", "async", ["input_lorem"]
)

__all__ = ["MyLorem", "add_three_to_field_a", "add_three_to_field_a_async"]
