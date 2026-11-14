package main

func task(i int) int { return i * 2 }

func Main() int {
	futures := []chan int{}
	for i := 0; i < 100; i++ {
		done := make(chan int, 1)
		go func() { done <- task(i) }()
		futures = append(futures, done)
	}
	s := 0
	for _, f := range futures {
		s += <-f
	}
	return s
}
