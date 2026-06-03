class User {
  name string
  age int
}

function ConstBindings(user: User, value: int | string) -> int {
  const x = 1;
  x = 2;
  const y: int = x;
  const User { age } = user;
  const shadowed = if (true) {
    const x = age;
    x
  } else {
    0
  };
  const n: int = value else { return 0; };
  let const = y;
  const + shadowed + n
}

//----
//- diagnostics
// <no-diagnostics-expected>
