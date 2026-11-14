package main

func gapply[T any](f func(T) T, x T) T {
	return f(x)
}

func Main() int {
	double := func(x int) int { return x * 2 }
	s := 0
	for i := 0; i < 1000000; i++ {
		s += gapply(double, i)
	}
	return s
}
