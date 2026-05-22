# `ns_symbol_collisions` — namespace symbol-collision regression coverage

Three classes named `Bar` coexist in this BAML project, each in a different
namespace path. Two consumers reach all three from outside, exercising the
codegen's resolution scheme for bare-name collisions across nested namespaces.

## Layout

```
ns_foo/types.baml                       class Bar { label, count }
ns_fizz/ns_foo/types.baml               class Bar { tag, ratio }
ns_fizz/ns_buzz/ns_foo/types.baml       class Bar { flavor, weight, active }
ns_lorem/uses.baml                      class Ipsum { bar1, bar2, bar3 }       — depth-1 consumer
ns_a/ns_b/ns_c/ns_d/uses.baml           class Deep  { here, there, further, nested } — depth-4 consumer
```

Each `Bar` has a deliberately different field shape so a codegen mix-up
shows up as a pydantic validation error, not silent data corruption.

`ns_fizz/` and `ns_fizz/ns_buzz/` contain no `.baml` files of their own —
they are pure pass-through namespaces. Reaching `fizz.foo.Bar` traverses
one pass-through; `fizz.buzz.foo.Bar` traverses two.

## What the consumers pin

`Ipsum` (depth 1, in `lorem/`):
- `bar1: symbol_collisions.foo.Bar` — sibling leaf, no pass-throughs.
- `bar2: symbol_collisions.fizz.foo.Bar` — one pass-through (`fizz`).
- `bar3: symbol_collisions.fizz.buzz.foo.Bar` — two pass-throughs (`fizz`, `buzz`).

`Deep` (depth 4, in `a/b/c/d/`):
- Same three `Bar` refs as `Ipsum`, plus
- `nested: symbol_collisions.lorem.Ipsum` — cross-leaf into another consumer.

The depth-4 site forces the codegen's relative-import dot count to
`depth + 2` (here, 6 dots: `from ...... import symbol_collisions`); an
off-by-one or a missing intermediate registration surfaces here first.

## What the codegen has to do to pass

Cross-namespace references emit a single relative import of the parent
leaf and reach every collision target through dotted attribute access —
no aliasing, no symbol renaming. Concretely, `lorem/__init__.py` ends up
with:

```python
from ... import symbol_collisions

class Ipsum(pydantic.BaseModel):
    bar1: symbol_collisions.foo.Bar
    bar2: symbol_collisions.fizz.foo.Bar
    bar3: symbol_collisions.fizz.buzz.foo.Bar
```

For that attribute walk to resolve at pydantic model-build time, every
intermediate pass-through `__init__.py` must register its children so
`getattr(fizz, "buzz")` and `getattr(buzz, "foo")` succeed. The current
scheme uses PEP 562 lazy resolution:

```python
# symbol_collisions/fizz/__init__.py
_LAZY_CHILDREN = frozenset({"buzz", "foo"})
def __getattr__(name):
    if name in _LAZY_CHILDREN:
        import importlib
        return importlib.import_module(f".{name}", __name__)
    raise AttributeError(...)
```

Drop an entry from `_LAZY_CHILDREN`, mis-count a dot, or rename one of
the `Bar`s during emission and the test crate's `pytest` step fails when
pydantic resolves `Ipsum`'s forward refs, or `pyright` fails on the
unresolved attribute access. Either signal points at the codegen, not at
the user code.
