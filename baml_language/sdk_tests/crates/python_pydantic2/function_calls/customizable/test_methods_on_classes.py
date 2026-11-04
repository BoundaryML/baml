"""Static + instance method coverage (ns_methods_on_classes.Greeter).

`raises_test.DocLoader` pins the *shape* of method bindings, but its bodies
always throw, so they are never invoked. `Greeter` has non-throwing bodies, so
this exercises the full host->engine round-trip for both flavors:

  - static   -> ``Greeter.create(name)``       (``staticmethod``, no ``self``)
  - instance -> ``g.who()`` / ``g.greet(arg)``  (``self`` is the first paramName)

each with its ``_async`` sibling. ``asyncio_mode = "auto"`` (set in build.rs)
runs ``async def test_*`` without an explicit decorator.

Node counterpart: ``crates/typescript/function_calls/customizable/methods_on_classes.test.ts``.
"""

import baml_sdk  # noqa: F401  — initializes the BAML runtime
from baml_sdk.methods_on_classes import Greeter


def test_methods_on_classes_method_bindings_exist():
    # Static bindings hang off the class; instance bindings carry `self`.
    assert callable(Greeter.create_sync)
    assert callable(Greeter.create)
    assert callable(Greeter.who_sync)
    assert callable(Greeter.who)
    assert callable(Greeter.greet_sync)
    assert callable(Greeter.greet)


def test_methods_on_classes_static_create_round_trips():
    g = Greeter.create_sync("ada")
    assert isinstance(g, Greeter)
    assert g.name == "ada"


async def test_methods_on_classes_static_create_async_round_trips():
    g = await Greeter.create("grace")
    assert isinstance(g, Greeter)
    assert g.name == "grace"


def test_methods_on_classes_instance_who_round_trips():
    g = Greeter.create_sync("hopper")
    assert g.who_sync() == "hopper"


async def test_methods_on_classes_instance_who_async_round_trips():
    g = await Greeter.create("hopper")
    assert await g.who() == "hopper"


def test_methods_on_classes_instance_greet_with_arg_round_trips():
    g = Greeter.create_sync("lovelace")
    assert g.greet_sync("hi") == "hi"


async def test_methods_on_classes_instance_greet_async_with_arg_round_trips():
    g = await Greeter.create("lovelace")
    assert await g.greet("hi") == "hi"
