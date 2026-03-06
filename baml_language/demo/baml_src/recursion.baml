// ── Recursive Functions ─────────────────────────────────────────

function Factorial(n: int) -> int {
    if (n < 2) {
        return 1;
    }
    return n * Factorial(n - 1);
}

function Fibonacci(n: int) -> int {
    if (n < 2) {
        return n;
    }
    return Fibonacci(n - 1) + Fibonacci(n - 2);
}

// ── Mutual Recursion ────────────────────────────────────────────

function IsEven(n: int) -> bool {
    if (n == 0) { return true; }
    return IsOdd(n - 1);
}

function IsOdd(n: int) -> bool {
    if (n == 0) { return false; }
    return IsEven(n - 1);
}
