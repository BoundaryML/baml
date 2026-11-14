package main

func leaf(n int) int {
	return n
}

func Main() int {
	s := 0
	for i := 0; i < 1000000; i++ {
		s += leaf(i)
	}
	return s
}
