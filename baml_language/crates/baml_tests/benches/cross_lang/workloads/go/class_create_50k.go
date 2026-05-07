package crosslang

type pointXY struct {
	x int
	y int
}

func ClassCreate50K() int {
	s := 0
	for i := 0; i < 50000; i++ {
		p := pointXY{x: i, y: i * 2}
		s += p.x + p.y
	}
	return s
}
