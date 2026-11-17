package main

import "time"

func nap() int {
	time.Sleep(200 * time.Millisecond)
	return 1
}

func spawnNap() chan int {
	done := make(chan int, 1)
	go func() { done <- nap() }()
	return done
}

func Main() int {
	a := spawnNap()
	b := spawnNap()
	c := spawnNap()
	return <-a + <-b + <-c
}
