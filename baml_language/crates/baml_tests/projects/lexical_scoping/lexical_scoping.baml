function branch_locals(b: bool) -> int {
  if (b) {
    let a = 1;
    a
  } else {
    let a = 2;
    a
  }
}

function repeated_underscore() -> int {
  let _ = 1;
  let _ = 2;
  _
}

function same_scope_shadow() -> int {
  let x = 1;
  let x = 2;
  x
}

function initializer_uses_previous() -> int {
  let x = 1;
  let x = x + 1;
  x
}

function shadow_param(x: int) -> int {
  let x = x + 1;
  x
}

function outer_restored() -> int {
  let x = 1;
  {
    let x = 2;
  };
  x
}

function declared_type_restored() -> string {
  let x: string = "outer";
  {
    let x: int = 1;
  };
  x
}

function for_loop_restores_outer() -> int {
  let x = 1;
  for (let x in [2, 3]) {
    x;
  };
  x
}

function for_underscore_binding() -> int {
  let _ = 1;
  for (let _ in [2, 3]) {
    _;
  };
  _
}

function watch_block_cleanup() -> int {
  watch let x = 1;
  {
    watch let x = 2;
    x;
  };
  x
}

function nested_outer_restored() -> int {
  let x = 1;
  {
    let x = 2;
    {
      let x = 3;
      x;
    };
    x;
  };
  x
}

function capture_before_after_shadow() -> int {
  let x = 1;
  let g = () -> int { x };
  let x = 2;
  let f = () -> int { x };
  g() * 10 + f()
}
