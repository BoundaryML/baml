# Due to tracing, we need to ensure we track context vars for each thread.
# This helps ensure we correctly instantiate the span and context for each thread.

import asyncio
import contextvars
import typing
from .baml_py import BamlSpan

ctx_tags = contextvars.ContextVar[
    typing.Tuple[typing.Optional[BamlSpan], typing.Dict[str, typing.Any]]
]("baml_ctx", default=(None, {}))


def upsert_tags(**tags: typing.Any) -> None:
    span, prev_tags = ctx_tags.get()
    prev_tags.update(tags)
    ctx_tags.set((span, prev_tags))
