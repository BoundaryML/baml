// throws never calling a non-throwing function — should be fine

function getProfile(req: int) -> bool {
  return true
}

function server(req: int) -> bool throws never {
  getProfile(req)
}

//----
//- diagnostics
// <no-diagnostics-expected>
