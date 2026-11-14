package main

func collatzSteps(n int) int {
	steps := 0
	x := n
	for x != 1 {
		if x%2 == 0 {
			x = x / 2
		} else {
			x = 3*x + 1
		}
		steps++
	}
	return steps
}

func Main() int {
	s := 0
	for i := 1; i < 100000; i++ {
		s += collatzSteps(i)
	}
	return s
}
