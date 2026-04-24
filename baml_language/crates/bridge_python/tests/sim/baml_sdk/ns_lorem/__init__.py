from __future__ import annotations

from baml.baml_core import define_function as __define_function

# Namespace segment follows the 09b §1 routing rule: the `ns_` prefix on
# the source directory is stripped, so `ns_lorem/root.baml` exports here
# under BAML FQN `root.lorem.*`. The bridge translates `root.` → `user.`
# before hitting the engine.

add_three_to_field_a = __define_function(
    "root.lorem.add_three_to_field_a", "sync", ["input_lorem"]
)
add_three_to_field_a_async = __define_function(
    "root.lorem.add_three_to_field_a", "async", ["input_lorem"]
)

__all__ = ["add_three_to_field_a", "add_three_to_field_a_async"]
