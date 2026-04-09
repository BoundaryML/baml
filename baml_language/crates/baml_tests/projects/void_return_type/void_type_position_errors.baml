// ERROR: void as parameter type
function bad_param(x: void) -> int {
  1
}

// ERROR: void as let annotation
function bad_let() -> int {
  let x: void = 1;
  1
}

// ERROR: void in optional wrapper
function bad_optional() -> void? {
  null
}

// ERROR: void in list
function bad_list() -> void[] {
  []
}
