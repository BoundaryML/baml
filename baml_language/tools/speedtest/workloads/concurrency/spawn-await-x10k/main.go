package main

func Main() int {
	s := 0
	for i := 0; i < 10000; i++ {
		done := make(chan int, 1)
		go func() { done <- 1 }()
		s += <-done
	}
	return s
}
