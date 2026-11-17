package main

func Main() int {
	s := 0
	for i := 0; i < 500; i++ {
		for j := 0; j < 500; j++ {
			s += i * j
		}
	}
	return s
}
