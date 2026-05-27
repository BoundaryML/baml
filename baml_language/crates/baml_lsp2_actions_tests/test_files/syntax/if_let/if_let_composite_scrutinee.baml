// `if let pat = func()` — composite scrutinee (call expression, not a
// path local). Bindings still work; no scrutinee-narrowing applies because
// there's no name to narrow.
class Ok {
  value string
}
class Err {
  message string
}

function maybe() -> Ok | Err {
  Ok { value: "computed" }
}

function CompositeScrutinee() -> string {
  if let o: Ok = maybe() {
    o.value
  } else {
    "no match"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
