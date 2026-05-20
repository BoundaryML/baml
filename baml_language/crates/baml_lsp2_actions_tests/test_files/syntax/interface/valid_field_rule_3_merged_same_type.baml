// BEP-044 interface field namespaces: two interfaces may declare the same
// field name with the same type, but the fields remain distinct namespaces.

interface Named {
  name: string
}

interface Labeled {
  name: string
}

class Item {
  implements Named {
    name: string
  }
  implements Labeled {
    name: string
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
