function WithParams(a: int) -> string {
  `Value: ${a}`
}

function GoodCall1() -> string {
  `${WithParams(a = 2)}`
}

function GoodCall2() -> string {
  `${WithParams(2)}`
}

//----
//- diagnostics
// <no-diagnostics-expected>
