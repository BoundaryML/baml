interface Named {
  function name(self) -> string throws never;
}

interface Tagged {
  function tag(self) -> string throws never;
}

function RuntimeTypes(t: reflect.Type, view: reflect.TypeView) -> string? {
  let type T = t;
  let type V = view;
  let type U extends Named = t else { return null };
  let type W extends Named & Tagged = t else {
    return null;
  };

  let typed: T[] = [];
  let items: map<string, U> = {};
  let back = reflect.Type.of<T>();
  let decoded = baml.json.to<V>(1);
  back.to_string()
}
