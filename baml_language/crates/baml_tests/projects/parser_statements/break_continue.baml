// Break statement in while loop
function simple_break() -> int {
  while (true) {
    break;
  }
  return 42;
}

// Continue statement in while loop
function simple_continue() -> int {
  while (true) {
    continue;
  }
  return 0;
}

// Break in for loop
function break_in_for() -> int {
  for (let i = 0; i < 10; i = i + 1) {
    break;
  }
  return 1;
}

// Continue in for loop
function continue_in_for() -> int {
  for (let i = 0; i < 10; i = i + 1) {
    continue;
  }
  return 2;
}

// Nested loops with break
function nested_break() -> int {
  while (true) {
    while (true) {
      break;
    }
    break;
  }
  return 3;
}

// Break with locals in nested scope - verifies scope drops work
function break_with_locals() -> int {
  while (true) {
    let x = 1;
    if (true) {
      let y = 2;
      break;  // Should pop y and x before jumping out
    }
  }
  return 4;
}

// Continue with locals - verifies scope drops for continue
function continue_with_locals() -> int {
  while (true) {
    let x = 1;
    if (true) {
      let y = 2;
      continue;  // Should pop y and x before jumping back
    }
  }
  return 5;
}
