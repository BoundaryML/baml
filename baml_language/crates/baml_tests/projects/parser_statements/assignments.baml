// Simple assignment
function simple_assign() -> int {
  let x = 1;
  x = 5;
  return x;
}

// Compound add assignment
function add_assign() -> int {
  let x = 10;
  x += 5;
  return x;
}

// Compound sub assignment
function sub_assign() -> int {
  let x = 10;
  x -= 3;
  return x;
}

// Compound mul assignment
function mul_assign() -> int {
  let x = 3;
  x *= 4;
  return x;
}

// Multiple assignments
function multi_assign() -> int {
  let a = 1;
  let b = 2;
  a = 10;
  b = 20;
  return a + b;
}

// Assignment in loop (uses break/continue we just implemented)
function assign_in_loop() -> int {
  let sum = 0;
  let i = 0;
  while (i < 5) {
    sum += i;
    i += 1;
  };
  return sum;
}

// Block expression with tail - tests PopReplace
function block_with_tail() -> int {
  let x = {
    let y = 1;
    let z = 2;
    y + z
  };
  return x;
}
