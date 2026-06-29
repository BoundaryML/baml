class User {
  name string
}

function BindingAndDeferForms(user: User, xs: int[], value: int | string, items: int[]) -> int {
  const x = 1;
  const y: int = x;
  const _ = y;
  const User { name } = user;
  const [first] = xs;

  let v: int = value else { return 0; };

  defer {
    let cleanup = v;
    cleanup;
  }

  watch let observed = v;

  if const narrowed: int = value {
    observed += narrowed;
  } else {
    observed += 0;
  };

  while const loop_value: int = value {
    observed += loop_value;
    break;
  }

  for (const i = 0; i < 3; i += 1) {
    observed += i;
  }

  for (const item in items) {
    observed += item;
  }

  for const item in items {
    observed += item;
  }

  observed
}
