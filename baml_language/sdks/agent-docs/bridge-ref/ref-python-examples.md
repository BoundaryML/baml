---
date: 2026-07-08
repository: baml4
source_fixtures:
  - baml_language/sdk_tests/fixtures/type_shapes/baml_src
  - baml_language/sdk_tests/crates/python_pydantic2/type_shapes/generated
  - baml_language/sdk_tests/fixtures/function_calls/baml_src
  - baml_language/sdk_tests/crates/python_pydantic2/function_calls/generated
---

# Python Codegen Examples From SDK Tests

This file records the Python prior art that the generated SDK tests already
exercise. It is the Python counterpart to `00a-prior-art-ts-examples.md`: where
that document sketches a proposed single-file TypeScript output, this document
describes the Python output as it exists today in the `python_pydantic2` SDK
test fixtures.

The snippets below are representative, not an exhaustive copy of every
generated namespace. They intentionally mirror the codegen's split between:

- runtime `.py` files, which initialize bytecode, install the type map, bind
  functions with `baml_bridge.define_function`, lazy-load child packages, and
  define runtime Pydantic models; and
- stub `.pyi` files, which expose the typed surface to type checkers.

## File Layout

Type-shape fixture paths:

```text
baml_sdk/__init__.py
baml_sdk/__init__.pyi
baml_sdk/_inlinedbaml.py
baml_sdk/_typemap.py
baml_sdk/py.typed
baml_sdk/primitives/__init__.py
baml_sdk/primitives/__init__.pyi
baml_sdk/symbol_collisions/lorem/__init__.py
baml_sdk/symbol_collisions/lorem/__init__.pyi
baml_sdk/stream_types/primitives/__init__.py
baml_sdk/stream_types/primitives/__init__.pyi
```

Function-call fixture paths follow the same package pattern:

```text
baml_sdk/__init__.py
baml_sdk/__init__.pyi
baml_sdk/methods_on_classes/__init__.py
baml_sdk/methods_on_classes/__init__.pyi
baml_sdk/raises_test/__init__.py
baml_sdk/raises_test/__init__.pyi
baml_sdk/host_callable_tests/__init__.py
baml_sdk/host_callable_tests/__init__.pyi
```

Each BAML namespace maps to a Python package directory with a generated
`__init__.py` and `__init__.pyi`. Child namespace symbols are not flattened
into their parent namespace; callers reach them through the child package.

For example, `baml_sdk.symbol_collisions` exposes `a`, `fizz`, `foo`, and
`lorem`, but `Ipsum` stays at `baml_sdk.symbol_collisions.lorem.Ipsum`:

```python
# baml_sdk/symbol_collisions/__init__.py
from __future__ import annotations

_LAZY_CHILDREN = frozenset({
    "a",
    "fizz",
    "foo",
    "lorem",
})

def __getattr__(name):
    if name in _LAZY_CHILDREN:
        import importlib
        return importlib.import_module(f".{name}", __name__)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
```

```python
# baml_sdk/symbol_collisions/__init__.pyi
from __future__ import annotations

from . import a
from . import fizz
from . import foo
from . import lorem
```

## Root Package

Derived from `type_shapes/generated/baml_sdk/__init__.py` and
`__init__.pyi`.

The runtime root initializes the embedded bytecode, installs the lazy type map,
and exposes child namespaces via PEP 562 `__getattr__`:

```python
from __future__ import annotations

from baml_bridge import BamlRuntime, set_type_map
from . import _inlinedbaml
from ._typemap import _TYPE_MAP

BamlRuntime.initialize_runtime_from_bytecode(_inlinedbaml.BYTECODE)

set_type_map(_TYPE_MAP)

_LAZY_CHILDREN = frozenset({
    "a",
    "aliases",
    "aliases_consumer",
    "baml",
    "class_refs",
    "complex_models",
    "enums",
    "forward_refs",
    "generics",
    "ipsum",
    "lists",
    "literals",
    "lorem",
    "maps",
    "media",
    "optional",
    "primitives",
    "recursion",
    "stream_types",
    "symbol_collisions",
    "unions",
    "vendor",
    "void",
})

def __getattr__(name):
    if name in _LAZY_CHILDREN:
        import importlib
        return importlib.import_module(f".{name}", __name__)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
```

Root-namespace user symbols are direct package exports:

```python
# __init__.py runtime surface
import pydantic
from baml_bridge import define_function as _define_function


class Foo(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    v: int


make_foo       = _define_function("user.make_foo", "sync",  ["v"])
make_foo_async = _define_function("user.make_foo", "async", ["v"])


round_trip_foo       = _define_function("user.round_trip_foo", "sync",  ["f"])
round_trip_foo_async = _define_function("user.round_trip_foo", "async", ["f"])
```

```python
# __init__.pyi typed surface
import pydantic


class Foo(pydantic.BaseModel):
    v: int


def make_foo(v: int) -> Foo: ...
async def make_foo_async(v: int) -> Foo: ...


def round_trip_foo(f: Foo) -> Foo: ...
async def round_trip_foo_async(f: Foo) -> Foo: ...
```

There is no generated `b` client object. The package itself is the client:

```python
import baml_sdk as b

foo = await b.make_foo_async(1)
n = await b.primitives.return_int_async()
ipsum = await b.symbol_collisions.lorem.make_ipsum_async(bar1, bar2, bar3)
```

Wrong: these would require child symbols to be hoisted into the root package:

```python
await b.return_int_async()
await b.make_ipsum_async(bar1, bar2, bar3)
```

## Primitive Namespace

Derived from `ns_primitives/types.baml` and `type_shapes/generated/baml_sdk/primitives`.

```python
# primitives/__init__.py
from __future__ import annotations

import pydantic
from baml_bridge import define_function as _define_function


class Primitives(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    int_field: int
    float_field: float
    string_field: str
    bool_field: bool
    null_field: None
    uint8array_field: bytes


return_int       = _define_function("user.primitives.return_int", "sync",  [])
return_int_async = _define_function("user.primitives.return_int", "async", [])

round_trip_uint8_array       = _define_function("user.primitives.round_trip_uint8_array", "sync",  ["b"])
round_trip_uint8_array_async = _define_function("user.primitives.round_trip_uint8_array", "async", ["b"])

round_trip_primitives       = _define_function("user.primitives.round_trip_primitives", "sync",  ["p"])
round_trip_primitives_async = _define_function("user.primitives.round_trip_primitives", "async", ["p"])
```

```python
# primitives/__init__.pyi
from __future__ import annotations

import pydantic


class Primitives(pydantic.BaseModel):
    int_field: int
    float_field: float
    string_field: str
    bool_field: bool
    null_field: None
    uint8array_field: bytes


def return_int() -> int: ...
async def return_int_async() -> int: ...

def round_trip_uint8_array(b: bytes) -> bytes: ...
async def round_trip_uint8_array_async(b: bytes) -> bytes: ...

def round_trip_primitives(p: Primitives) -> Primitives: ...
async def round_trip_primitives_async(p: Primitives) -> Primitives: ...
```

The runtime file does not put callable type signatures on the function
bindings. The `.pyi` file is the source of the typed callable surface.

## Enums And Aliases

Enums are `str, enum.Enum` classes:

```python
class Sentiment(str, enum.Enum):
    Positive = "Positive"
    Negative = "Negative"


class Enums(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    bare_enum: Sentiment
    variant_as_type: Sentiment
```

Specific enum variant types are widened to the enum class in Python. In the
fixture, `variant_as_type` and `round_trip_sentiment_positive` both type as
`Sentiment`.

Non-recursive aliases use `typing.TypeAlias`; recursive aliases use
`typing_extensions.TypeAliasType` with quoted forward references:

```python
RecList = typing_extensions.TypeAliasType(
    "RecList",
    typing.Union[int, typing.List["RecList"]],
)

StringList: typing.TypeAlias = typing.List[str]


class AliasContainer(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    list_field: typing.List[str]
    rec_field: RecList
```

## Generics And Instance Methods

Derived from `ns_generics/types.baml`, `ns_generics/methods.baml`, and
`type_shapes/generated/baml_sdk/generics`.

Generic classes inherit from `pydantic.BaseModel` and `typing.Generic[T]`.
The runtime file declares `TypeVar`s in the module and binds BAML methods in
the class body:

```python
T = typing.TypeVar("T")


class WrapperMethods(pydantic.BaseModel, typing.Generic[T]):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: T
    get_value       = _define_function("user.generics.WrapperMethods.get_value", "sync",  ["self"], class_type_params=["T"])
    get_value_async = _define_function("user.generics.WrapperMethods.get_value", "async", ["self"], class_type_params=["T"])
    get_value_or_marker       = _define_function("user.generics.WrapperMethods.get_value_or_marker", "sync",  ["self"], class_type_params=["T"])
    get_value_or_marker_async = _define_function("user.generics.WrapperMethods.get_value_or_marker", "async", ["self"], class_type_params=["T"])


class WrapperMarker(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    reason: str


class Wrapper(pydantic.BaseModel, typing.Generic[T]):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: T


class Box(pydantic.BaseModel, typing.Generic[T]):
    model_config = pydantic.ConfigDict(extra="forbid")
    value: T
    wrapped: Wrapper[T]
```

The stub presents those runtime bindings as normal methods:

```python
class WrapperMethods(pydantic.BaseModel, typing.Generic[T]):
    value: T
    def get_value(self) -> T: ...
    async def get_value_async(self) -> T: ...
    def get_value_or_marker(self) -> typing.Union[T, WrapperMarker]: ...
    async def get_value_or_marker_async(self) -> typing.Union[T, WrapperMarker]: ...


def make_wrapper_methods(text: str) -> WrapperMethods[str]: ...
async def make_wrapper_methods_async(text: str) -> WrapperMethods[str]: ...


def round_trip_box_int(b: Box[int]) -> Box[int]: ...
async def round_trip_box_int_async(b: Box[int]) -> Box[int]: ...
```

Static methods use `staticmethod(...)` in the runtime file and `@staticmethod`
in the stub. Instance methods include `"self"` in the runtime parameter-name
array, but `self` is not a user argument in the stub.

Callables on (or returning) generic types carry their generic parameters as
keyword-only args to `_define_function`: a method on a generic class gets
`class_type_params=["T", ...]` (the enclosing class's params), and a callable
with its own `<...>` params gets `type_params=[...]`. These are what let the
runtime map an explicit `_types={T: int}` kwarg (or a `fn[int](...)` subscript)
onto `CallFunctionArgs.type_args`. The corresponding `.pyi` also gains a
keyword-only `_types: dict[str, type]` param.

## Cross-Namespace References

Derived from `ns_symbol_collisions/ns_lorem/uses.baml`.

Runtime files import the nearest package namespace unconditionally when class
annotations must resolve at module load or Pydantic schema-build time:

```python
# symbol_collisions/lorem/__init__.py
from __future__ import annotations

import pydantic
from ... import symbol_collisions
from baml_bridge import define_function as _define_function


class Ipsum(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    bar1: symbol_collisions.foo.Bar
    bar2: symbol_collisions.fizz.foo.Bar
    bar3: symbol_collisions.fizz.buzz.foo.Bar


make_ipsum = _define_function(
    "user.symbol_collisions.lorem.make_ipsum",
    "sync",
    ["bar1", "bar2", "bar3"],
)
```

The stub can guard that import under `typing.TYPE_CHECKING`:

```python
# symbol_collisions/lorem/__init__.pyi
from __future__ import annotations

import typing
import pydantic

if typing.TYPE_CHECKING:
    from ... import symbol_collisions


class Ipsum(pydantic.BaseModel):
    bar1: symbol_collisions.foo.Bar
    bar2: symbol_collisions.fizz.foo.Bar
    bar3: symbol_collisions.fizz.buzz.foo.Bar
```

The namespace boundary is still preserved. `Ipsum` is not exported from
`symbol_collisions/__init__.pyi`; callers use
`baml_sdk.symbol_collisions.lorem.Ipsum`.

## Stdlib Re-Exports

Most generated symbols are emitted as Pydantic models, enums, aliases, or
function bindings. Handle-backed stdlib media types and `baml.llm.Stream` are
exceptions: the generated package re-exports runtime-owned classes.

```python
# baml/media/__init__.py and __init__.pyi
from baml_bridge.baml_py import BamlPdf as Pdf
from baml_bridge.baml_py import BamlAudio as Audio
from baml_bridge.baml_py import BamlVideo as Video
from baml_bridge.baml_py import BamlImage as Image
```

`baml/llm`, by contrast, is *not* just a re-export shim — it is a full
generated namespace (`Client`, `PrimitiveClient`, `Context`, `ClientType`,
`RetryPolicy`, `StreamAccumulator`, provider option models, `render_prompt`,
`build_request`, etc., where `Client` is itself a Pydantic model). The
runtime-owned `Stream` re-export lives inside it alongside the generated
symbols:

```python
# baml/llm/__init__.py and __init__.pyi (excerpt — one line among many)
from baml_bridge import BamlStream as Stream
```

The public paths are still generated package paths such as
`baml_sdk.baml.media.Image` and `baml_sdk.baml.llm.Stream`, but those two value
classes come from `baml_bridge`; most of `baml.llm` is generated code.

## Stream Companion Types

Python keeps stream companion types in a real `stream_types` package tree.
The BAML FQN retains `$stream`, but the Python class name is the base name in
the `stream_types` module because `$` is not a Python identifier.

The type map makes this explicit:

```python
"user.primitives.Primitives": ("baml_sdk.primitives", "Primitives"),
"user.primitives.Primitives$stream": ("baml_sdk.stream_types.primitives", "Primitives"),
"user.generics.Wrapper": ("baml_sdk.generics", "Wrapper"),
"user.generics.Wrapper$stream": ("baml_sdk.stream_types.generics", "Wrapper"),
```

Example stream companion stub:

```python
# stream_types/primitives/__init__.pyi
from __future__ import annotations

import typing
import pydantic


class Primitives(pydantic.BaseModel):
    int_field: typing.Optional[int]
    float_field: typing.Optional[float]
    string_field: typing.Optional[str]
    bool_field: typing.Optional[bool]
    null_field: None
    uint8array_field: bytes
```

Generic stream companion stubs mirror the compiler-produced companion shape.
They do not contain static functions or instance methods:

```python
class WrapperMethods(pydantic.BaseModel, typing.Generic[T]):
    value: typing.Optional[T]


class Box(pydantic.BaseModel, typing.Generic[T]):
    value: typing.Optional[T]
    wrapped: typing.Optional[Wrapper[T]]
```

Codegen consumes the compiler-produced `$stream` class shape as a regular
class. It does not derive a generic `Partial[T]` transformation at Python
codegen time.

## Optional Function Arguments

Derived from `function_calls/baml_src/main.baml` and
`function_calls/generated/baml_sdk/__init__.py`.

Required arguments remain positional. Optional BAML arguments become
keyword-only in `.pyi` stubs. Literal defaults are emitted as the literal
Python default; expression defaults that must be evaluated by the engine use
`UNSET`.

`UNSET` is a [PEP 661](https://peps.python.org/pep-0661/) sentinel built in
`baml_bridge` as `UNSET = typing_extensions.Sentinel("UNSET")`. A `Sentinel`
value doubles as its own type, so a single object serves both the annotation
and the default: `opt: typing.Union[int, None, UNSET] = UNSET`. There is no
longer a separate `Unset` class.

Type checkers only recognize a sentinel in a type expression when it is
referenced by a **bare name**, not via attribute access — `baml.UNSET` in a
type position is rejected. So a leaf with optional arguments imports the
sentinel directly (under `typing.TYPE_CHECKING` in the `.pyi`) and references
it unqualified:

```python
if typing.TYPE_CHECKING:
    from ..baml import UNSET as UNSET
```

The default-value position is an ordinary expression, so attribute access
(`baml.UNSET`) would also work there; codegen uses the bare `UNSET` uniformly.
At runtime the value is the single re-exported `baml_bridge.UNSET`, and
`_build_kwargs` drops any argument whose value `is UNSET`, so `UNSET` (omit the
argument) stays distinct from `None` (pass an explicit null). Recognizing a
sentinel in a type expression requires `typing-extensions >= 4.13` and
`pyright >= 1.1.410`.

```python
# __init__.py runtime
optional_args_probe       = _define_function("user.optional_args_probe", "sync",  ["arg0"], ["opt1", "opt2"])
optional_args_probe_async = _define_function("user.optional_args_probe", "async", ["arg0"], ["opt1", "opt2"])


class OptBox(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    base: int
    make       = staticmethod(_define_function("user.OptBox.make", "sync",  ["base"], ["opt1"]))
    make_async = staticmethod(_define_function("user.OptBox.make", "async", ["base"], ["opt1"]))
    probe       = _define_function("user.OptBox.probe", "sync",  ["self", "arg0"], ["opt1"])
    probe_async = _define_function("user.OptBox.probe", "async", ["self", "arg0"], ["opt1"])
```

```python
# __init__.pyi typed surface
def optional_args_probe(
    arg0: int,
    *,
    opt1: typing.Union[int, None, UNSET] = 5,
    opt2: typing.Union[int, None, UNSET] = UNSET,
) -> typing.List[typing.Optional[int]]: ...


class OptBox(pydantic.BaseModel):
    base: int
    @staticmethod
    def make(base: int, *, opt1: typing.Union[int, None, UNSET] = 7) -> OptBox: ...
    @staticmethod
    async def make_async(base: int, *, opt1: typing.Union[int, None, UNSET] = 7) -> OptBox: ...
    def probe(self, arg0: int, *, opt1: typing.Union[int, None, UNSET] = 5) -> typing.List[typing.Optional[int]]: ...
    async def probe_async(self, arg0: int, *, opt1: typing.Union[int, None, UNSET] = 5) -> typing.List[typing.Optional[int]]: ...
```

The runtime `define_function` call receives two parameter-name arrays when
optional arguments exist: required names first, optional names second.

## Static And Instance Methods

Derived from `function_calls/baml_src/ns_methods_on_classes/types.baml`.

```python
# methods_on_classes/__init__.py
class Greeter(pydantic.BaseModel):
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    create       = staticmethod(_define_function("user.methods_on_classes.Greeter.create", "sync",  ["name"]))
    create_async = staticmethod(_define_function("user.methods_on_classes.Greeter.create", "async", ["name"]))
    who       = _define_function("user.methods_on_classes.Greeter.who", "sync",  ["self"])
    who_async = _define_function("user.methods_on_classes.Greeter.who", "async", ["self"])
    greet       = _define_function("user.methods_on_classes.Greeter.greet", "sync",  ["self", "greeting"])
    greet_async = _define_function("user.methods_on_classes.Greeter.greet", "async", ["self", "greeting"])
```

```python
# methods_on_classes/__init__.pyi
class Greeter(pydantic.BaseModel):
    name: str
    @staticmethod
    def create(name: str) -> Greeter: ...
    @staticmethod
    async def create_async(name: str) -> Greeter: ...
    def who(self) -> str: ...
    async def who_async(self) -> str: ...
    def greet(self, greeting: str) -> str: ...
    async def greet_async(self, greeting: str) -> str: ...
```

The runtime method values are descriptors produced by `define_function`.
There is no hand-written method body that delegates to a separate binding.

## Throws Docstrings

Derived from `function_calls/baml_src/ns_raises_test/types.baml` and
`function_calls/generated/baml_sdk/raises_test`.

Thrown types are documented, not encoded in return types. Generated function
signatures still return the success type.

```python
# raises_test/__init__.pyi
class ParseError(pydantic.BaseModel):
    message: str


class TimeoutError(pydantic.BaseModel):
    ms: int


def LoadDoc(path: str) -> str:
    """Load a document from a path.

    Raises:
        ParseError, TimeoutError"""
async def LoadDoc_async(path: str) -> str:
    """Load a document from a path.

    Raises:
        ParseError, TimeoutError"""
```

Runtime free-function bindings receive matching `__doc__` strings:

```python
LoadDoc       = _define_function("user.raises_test.LoadDoc", "sync",  ["path"])
LoadDoc.__doc__ = """Load a document from a path.

Raises:
    ParseError, TimeoutError"""
LoadDoc_async = _define_function("user.raises_test.LoadDoc", "async", ["path"])
LoadDoc_async.__doc__ = """Load a document from a path.

Raises:
    ParseError, TimeoutError"""
```

Non-throwing functions have no generated `Raises:` block. Inferred throws
contracts also appear in docstrings, even when the BAML function has no written
`throws` clause.

## Host Callable Types

Derived from `function_calls/baml_src/ns_host_callable_tests/main.baml`.

Python callable types render as `typing.Callable[[...], Ret]` in stubs:

```python
def call_with_callback(callback: typing.Callable[[int], str], x: int) -> str: ...
async def call_with_callback_async(callback: typing.Callable[[int], str], x: int) -> str: ...


def call_with_two_args(callback: typing.Callable[[int, str], str], x: int, prefix: str) -> str: ...
async def call_with_two_args_async(callback: typing.Callable[[int, str], str], x: int, prefix: str) -> str: ...


class Person(pydantic.BaseModel):
    name: str
    age: int


def call_with_class_callback(callback: typing.Callable[[Person], str], p: Person) -> str: ...
async def call_with_class_callback_async(callback: typing.Callable[[Person], str], p: Person) -> str: ...
```

The runtime file binds those exactly like ordinary free functions:

```python
call_with_callback       = _define_function("user.host_callable_tests.call_with_callback", "sync",  ["callback", "x"])
call_with_callback_async = _define_function("user.host_callable_tests.call_with_callback", "async", ["callback", "x"])
```

If a host-callable path propagates a typed throw, the generated Python surface
again documents it with a `Raises:` docstring rather than changing the return
type:

```python
def call_with_typed_throws_propagating(callback: typing.Callable[[int], str], x: int) -> str:
    """Raises:
        ValidationError"""
```

## Packaging Markers

The generated Python SDK root includes:

- `_inlinedbaml.py`, which stores compiler-produced bytecode/source payloads;
- `_typemap.py`, which builds `BamlTypeMap.from_lazy_entries(...)` for classes,
  enums, and aliases, including `$stream` FQNs routed into `stream_types`; and
- `py.typed`, marking the generated package as PEP 561 typed.

The test fixtures' `pyproject.toml` depends on `baml_bridge`, `pydantic>=2`, and
`typing-extensions`, and excludes `.pyi` files from Ruff formatting.
