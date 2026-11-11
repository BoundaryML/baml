type Name = string;
interface Named {
 type Key;
 type Error = never;
 name: Name,
 function label(self) -> string throws never;
 function display(self) -> string throws never { self.label() }
}
class Item {
 name: Name,
 implements Named { type Key = string;
 function label(self) -> string { self.name } }
}