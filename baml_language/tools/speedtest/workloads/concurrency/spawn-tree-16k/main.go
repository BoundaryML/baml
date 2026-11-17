package main

func tree(depth int) int {
	if depth == 0 {
		return 1
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
