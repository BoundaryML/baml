package main

func sumRange(start, end int) int {
	s := 0
	for i := start; i < end; i++ {
		s += i
	}
	return s
}

func spawnSum(start, end int) chan int {
	done := make(chan int, 1)
	go func() { done <- sumRange(start, end) }()
	return done
}

func Main() int {
	a := spawnSum(0, 250000)
	b := spawnSum(250000, 500000)
	c := spawnSum(500000, 750000)
	d := spawnSum(750000, 1000000)
	return <-a + <-b + <-c + <-d
}
