class Foo {
  value int
}

enum Color {
  Red
  Blue
}

interface Marker {}

type Callback = (value: int) -> bool throws never

function classify(t: reflect.Type) -> string {
  match (t.kind()) {
    reflect.class.Type => "class",
    reflect.enum.Type => "enum",
    reflect.union.Type => "union",
    reflect.literal.Type => "literal",
    reflect.array.Type => "array",
    reflect.map.Type => "map",
    reflect.interface.Type => "interface",
    reflect.primitive.Type => "primitive",
    reflect.function.Type => "function"
  }
}

function read_class(view: reflect.class.Type) -> reflect.Type throws never {
  view.as_type()
}

function exercise_all_kinds() -> bool {
  classify(reflect.Type.of<Foo>()) == "class"
    && classify(reflect.Type.of<Color>()) == "enum"
    && classify(reflect.Type.of<int | string>()) == "union"
    && classify(reflect.Type.of<"fixed">()) == "literal"
    && classify(reflect.Type.of<Foo[]>()) == "array"
    && classify(reflect.Type.of<map<string, Foo>>()) == "map"
    && classify(reflect.Type.of<Marker>()) == "interface"
    && classify(reflect.Type.of<int>()) == "primitive"
    && classify(reflect.Type.of<Callback>()) == "function"
}
