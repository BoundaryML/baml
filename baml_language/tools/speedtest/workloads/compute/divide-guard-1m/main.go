package main

func maybeDivide(a, b int) int {
	if b == 0 {
		return 0
	}
	return a / b
}

func Main() int {
	s := 0
	for i := 0; i < 1000000; i++ {
		s += maybeDivide(i, i+1)
	}
	return s
}
