package main

func Main() int {
	s := 0
	for i := 0; i < 1000000; i++ {
		s += i
	}
	return s
}
