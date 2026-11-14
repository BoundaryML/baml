package main

func classify(x int) int {
	if x%5 == 0 {
		return 1
	} else if x%5 == 1 {
		return 2
	} else if x%5 == 2 {
		return 3
	} else if x%5 == 3 {
		return 4
	} else {
		return 5
	}
}

func Main() int {
	s := 0
	for i := 0; i < 1000000; i++ {
		s += classify(i)
	}
	return s
}
