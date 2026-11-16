interface Named {
  type Key = string
  function label(self) -> string
}

interface Printable {
  function display(self) -> string
}

class Ticket {
  id: string
  function label(self) -> string { return self.id }
  implements Named {}
}

class Box<T> {
  value: T
}

implements<T extends Named> Printable for Box<T> {
  function display(self) -> string {
    return self.value.label()
  }
}
