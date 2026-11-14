package main

import "time"

func nap() int {
	time.Sleep(100 * time.Millisecond)
	return 1
}

func tree(depth int) int {
	if depth == 0 {
		return nap()
	}
	left := make(chan int, 1)
	right := make(chan int, 1)
	go func() { left <- tree(depth - 1) }()
	go func() { right <- tree(depth - 1) }()
	return <-left + <-right + 1
}

func Main() int {
	return tree(13)
}
