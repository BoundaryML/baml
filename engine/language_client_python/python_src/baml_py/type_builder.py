import typing
from .baml_py import (
    ClassBuilder,
    EnumBuilder,
    FieldType,
    ClassPropertyBuilder as _ClassPropertyBuilder,
    EnumValueBuilder,
    TypeBuilder as _TypeBuilder,
)


class TypeBuilder:
    def __init__(self, classes: typing.Set[str], enums: typing.Set[str]):
        self.__classes = classes
        self.__enums = enums
        self.__tb = _TypeBuilder()

    @property
    def _tb(self) -> _TypeBuilder:
        return self.__tb

    def string(self):
        return self._tb.string()

    def int(self):
        return self._tb.int()

    def float(self):
        return self._tb.float()

    def bool(self):
        return self._tb.bool()

    def list(self, inner: FieldType):
        return self._tb.list(inner)

    def add_class(self, name: str) -> "NewClassBuilder":
        if name in self.__classes:
            raise ValueError(f"Class with name {name} already exists.")
        if name in self.__enums:
            raise ValueError(f"Enum with name {name} already exists.")
        self.__classes.add(name)
        return NewClassBuilder(self._tb, name)

    def add_enum(self, name: str) -> "NewEnumBuilder":
        if name in self.__classes:
            raise ValueError(f"Class with name {name} already exists.")
        if name in self.__enums:
            raise ValueError(f"Enum with name {name} already exists.")
        self.__enums.add(name)
        return NewEnumBuilder(self._tb, name)


class NewClassBuilder:
    def __init__(self, tb: _TypeBuilder, name: str):
        self.__bldr = tb.class_(name)
        self.__properties: typing.Set[str] = set()
        self.__props = NewClassProperties(self.__bldr, self.__properties)

    def type(self) -> FieldType:
        return self.__bldr.field()

    def list_properties(self) -> typing.List[typing.Tuple[str, "ClassPropertyBuilder"]]:
        return [
            (name, ClassPropertyBuilder(self.__bldr.property(name)))
            for name in self.__properties
        ]

    def add_property(self, name: str, type: FieldType) -> "ClassPropertyBuilder":
        if name in self.__properties:
            raise ValueError(f"Property {name} already exists.")
        return ClassPropertyBuilder(self.__bldr.property(name).type(type))

    @property
    def props(self) -> "NewClassProperties":
        return self.__props


class ClassPropertyBuilder:
    def __init__(self, bldr: _ClassPropertyBuilder):
        self.__bldr = bldr

    def alias(self, alias: typing.Optional[str]):
        self.__bldr.alias(alias)
        return self

    def description(self, description: typing.Optional[str]):
        self.__bldr.description(description)
        return self


class NewClassProperties:
    def __init__(self, cls_bldr: ClassBuilder, properties: typing.Set[str]):
        self.__bldr = cls_bldr
        self.__properties = properties

    def __getattr__(self, name: str) -> "ClassPropertyBuilder":
        if name not in self.__properties:
            raise AttributeError(f"Property {name} not found.")
        return ClassPropertyBuilder(self.__bldr.property(name))


class NewEnumBuilder:
    def __init__(self, tb: _TypeBuilder, name: str):
        self.__bldr = tb.enum(name)
        self.__values: typing.Set[str] = set()
        self.__vals = NewEnumValues(self.__bldr, self.__values)

    def type(self) -> FieldType:
        return self.__bldr.field()

    @property
    def values(self) -> "NewEnumValues":
        return self.__vals

    def list_values(self) -> typing.List[typing.Tuple[str, EnumValueBuilder]]:
        return [(name, self.__bldr.value(name)) for name in self.__values]

    def add_value(self, name: str) -> "EnumValueBuilder":
        if name in self.__values:
            raise ValueError(f"Value {name} already exists.")
        self.__values.add(name)
        return self.__bldr.value(name)


class NewEnumValues:
    def __init__(self, enum_bldr: EnumBuilder, values: typing.Set[str]):
        self.__bldr = enum_bldr
        self.__values = values

    def __getattr__(self, name: str) -> "EnumValueBuilder":
        if name not in self.__values:
            raise AttributeError(f"Value {name} not found.")
        return self.__bldr.value(name)
