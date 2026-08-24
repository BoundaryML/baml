# interfaces::field access 100k

Interface **field** reads through an existential receiver. The three sibling
workloads all measure interface *method* dispatch, which was already open-world;
this is the path that changed when the compile-time implementor switch was
replaced by `virtual_load_field`, so it is the one whose cost moved.

Two implementors with the field at *different* slots, so the read genuinely
depends on the receiver's impl rather than a shared layout.

## BAML
```baml
interface Named {
  name: string
}
class Dog {
  name: string
  age: int
  implements Named {}
}
class Robot {
  serial: int
  model: string
  implements Named {
    name as model
  }
}
function pick(i: int) -> Named {
  if i % 2 == 0 { return Dog { name: "rex", age: i }; };
  return Robot { serial: i, model: "r2" };
}
function name_len(n: Named) -> int {
  return n.name.length()
}
function main() -> int {
  let acc = 0;
  for (let i = 0; i < 100000; i += 1) {
    acc += name_len(pick(i));
  };
  return acc
}
```
