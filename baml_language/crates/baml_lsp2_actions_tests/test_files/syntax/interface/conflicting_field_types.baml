// Same field name from unrelated interfaces is legal: each interface owns its
// own namespace, so `HasId.id` and `HasNumId.id` are distinct fields.

interface HasId {
  id: string
}

interface HasNumId {
  id: int
}

class Thing {
  text_id: string
  numeric_id: int

  implements HasId {
    id as text_id
  }
  implements HasNumId {
    id as numeric_id
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
