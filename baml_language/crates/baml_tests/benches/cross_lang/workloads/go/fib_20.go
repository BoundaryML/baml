package crosslang

func fib(n int) int {
	if n <= 1 {
		return n
	}
	return fib(n-1) + fib(n-2)
}

// Fib20 is shared between fib_20 and fib_20_e2e workloads.
func Fib20() int { return fib(20) }
