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
    reflect.Class => "class",
    reflect.Enum => "enum",
    reflect.Union => "union",
    reflect.Literal => "literal",
    reflect.Array => "array",
    reflect.Map => "map",
    reflect.Interface => "interface",
    reflect.Primitive => "primitive",
    reflect.Function => "function",
    reflect.Tuple => "tuple"
  }
}

function read_class(view: reflect.Class) -> reflect.Type throws never {
  view.as_type()
}

function exercise_all_kinds() -> bool {
  classify(Foo.type()) == "class"
    && classify(Color.type()) == "enum"
    && classify(reflect.Type.of<int | string>()) == "union"
    && classify(reflect.Type.of<"fixed">()) == "literal"
    && classify(reflect.Type.of<Foo[]>()) == "array"
    && classify(reflect.Type.of<map<string, Foo>>()) == "map"
    && classify(reflect.Type.of<Marker>()) == "interface"
    && classify(int.type()) == "primitive"
    && classify(reflect.Type.of<Callback>()) == "function"
    && classify(reflect.Type.of<(int, string)>()) == "tuple"
}
