// Native backend fixture: self and mutual recursion, and the depth limit.

function fact(n: int) -> int { if (n <= 1) { 1 } else { n * fact(n - 1) } }

function is_even(n: int) -> bool { if (n == 0) { true } else { is_odd(n - 1) } }

function is_odd(n: int) -> bool { if (n == 0) { false } else { is_even(n - 1) } }

function deep(n: int) -> int { if (n == 0) { 0 } else { deep(n - 1) + 1 } }

function fib(n: int) -> int { if (n < 2) { n } else { fib(n - 1) + fib(n - 2) } }
