package main

import "sync"

// shared is an int array that tasks may push to concurrently: every operation
// takes the lock, as BAML's runtime does for its arrays.
type shared struct {
	mu sync.Mutex
	xs []int
}

func (a *shared) push(v int) {
	a.mu.Lock()
	a.xs = append(a.xs, v)
	a.mu.Unlock()
}

func (a *shared) length() int {
	a.mu.Lock()
	n := len(a.xs)
	a.mu.Unlock()
	return n
}

func pushN(arr *shared, start, n int) int {
	for i := 0; i < n; i++ {
		arr.push(start + i)
	}
	return 0
}

func spawn(task func() int) chan int {
	done := make(chan int, 1)
	go func() { done <- task() }()
	return done
}

func Main() int {
	arr := &shared{xs: []int{}}
	a := spawn(func() int { return pushN(arr, 0, 25000) })
	b := spawn(func() int { return pushN(arr, 25000, 25000) })
	c := spawn(func() int { return pushN(arr, 50000, 25000) })
	d := spawn(func() int { return pushN(arr, 75000, 25000) })
	_ = <-a + <-b + <-c + <-d
	return arr.length()
}
