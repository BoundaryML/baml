package main

import "sync"

// shared is an int array that tasks may use concurrently: every operation
// takes the lock, as BAML's runtime does for its arrays.
type shared struct {
	mu sync.RWMutex
	xs []int
}

func (a *shared) push(v int) {
	a.mu.Lock()
	a.xs = append(a.xs, v)
	a.mu.Unlock()
}

func (a *shared) length() int {
	a.mu.RLock()
	n := len(a.xs)
	a.mu.RUnlock()
	return n
}

func (a *shared) at(i int) int {
	a.mu.RLock()
	v := a.xs[i]
	a.mu.RUnlock()
	return v
}

func writeN(arr *shared, n int) int {
	for i := 0; i < n; i++ {
		arr.push(i)
	}
	return 0
}

func readN(arr *shared, n int) int {
	acc := 0
	for i := 0; i < n; i++ {
		if arr.length() > 0 {
			acc += arr.at(0)
		}
	}
	return acc
}

func spawn(task func() int) chan int {
	done := make(chan int, 1)
	go func() { done <- task() }()
	return done
}

func Main() int {
	arr := &shared{xs: []int{0}}
	w := spawn(func() int { return writeN(arr, 50000) })
	r1 := spawn(func() int { return readN(arr, 12500) })
	r2 := spawn(func() int { return readN(arr, 12500) })
	r3 := spawn(func() int { return readN(arr, 12500) })
	r4 := spawn(func() int { return readN(arr, 12500) })
	_ = <-w + <-r1 + <-r2 + <-r3 + <-r4
	return arr.length()
}
