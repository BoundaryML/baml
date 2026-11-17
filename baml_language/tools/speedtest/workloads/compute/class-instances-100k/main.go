package main

type Point struct {
	x, y int
}

func Main() int {
	s := 0
	for i := 0; i < 100000; i++ {
		p := &Point{x: i, y: i * 2}
		s += p.x + p.y
	}
	return s
}
