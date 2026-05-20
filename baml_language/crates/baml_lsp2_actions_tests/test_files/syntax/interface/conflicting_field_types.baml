// Same field name from unrelated interfaces is legal: each interface owns its
// own namespace, so `HasId.id` and `HasNumId.id` are distinct fields.

interface HasId {
  id: string
}

interface HasNumId {
  id: int
}

class Thing {
  implements HasId {
    id: string
  }
  implements HasNumId {
    id: int
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
