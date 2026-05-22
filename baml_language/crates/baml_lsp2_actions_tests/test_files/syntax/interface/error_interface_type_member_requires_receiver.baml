// BEP-044: interface members are instance/view members. Use an interface-typed
// value or `.as<Interface>` projection, not `Interface.method(value)`.

interface Describable {
  function describe(self) -> string {
    return "default"
  }
}

class Thing {
  implements Describable {}
}

function bad() -> string {
  let t = Thing {}
  return Describable.describe(t)
}

//----
//- diagnostics
// error: interface member `describe` on `Describable` must be accessed through a value; use value.as<Describable>.describe
//     ╭─[ interface_error_interface_type_member_requires_receiver.baml:16:9 ]
//     │
//  16 │   return Describable.describe(t)
//     │         ──────────┬──────────  
//     │                   ╰──────────── interface member `describe` on `Describable` must be accessed through a value; use value.as<Describable>.describe
//     │ 
//     │ Note: Error code: E0007
// ────╯
