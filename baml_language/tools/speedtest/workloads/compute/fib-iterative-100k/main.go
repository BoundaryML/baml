package main

func fib(n int) int {
	a := 0
	b := 1
	for i := 0; i < n; i++ {
		tmp := b
		b = a + b
		a = tmp
	}
	return a
}

func Main() int {
	s := 0
	for i := 0; i < 100000; i++ {
		s += fib(50)
	}
	return s
}
