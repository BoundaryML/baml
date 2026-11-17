package main

func apply(f func(int) int, x int) int {
	return f(x)
}

func Main() int {
	double := func(x int) int { return x * 2 }
	s := 0
	for i := 0; i < 1000000; i++ {
		s += apply(double, i)
	}
	return s
}
