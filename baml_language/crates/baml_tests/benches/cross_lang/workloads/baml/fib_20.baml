function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(20) }
