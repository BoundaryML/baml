type Alias = (x: int) -> int

class Holder {
  cb (value: int) -> int
  opt_cb ((value: int) -> int)?
}

function let_annotation() -> int {
  let f: (x: int) -> int = (x: int) -> int { x };
  f(1)
}

function nested_param(cb: ((value: int) -> int) -> int) -> int {
  cb((value: int) -> int { value })
}

function optional_param(cb: ((value: int) -> int)?) -> int {
  1
}

function list_param(cbs: ((value: int) -> int)[]) -> int {
  0
}

function returns_fn() -> (value: int) -> int {
  (value: int) -> int { value }
}

function lambda_param() -> int {
  let apply = (f: (value: int) -> int, x: int) -> int { f(x) };
  apply((value: int) -> int { value }, 1)
}

function match_pattern(cb: (value: int) -> int) -> int {
  match (cb) {
    let f: (value: int) -> int => f(1)
  }
}
