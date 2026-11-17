package main

func validate(x int) int {
	if x < 0 {
		return -1
	}
	if x > 1000000 {
		return -2
	}
	if x%7 == 0 {
		return x * 2
	}
	if x%3 == 0 {
		return x + 1
	}
	return x
}

func Main() int {
	s := 0
	for i := 0; i < 1000000; i++ {
		s += validate(i)
	}
	return s
}
