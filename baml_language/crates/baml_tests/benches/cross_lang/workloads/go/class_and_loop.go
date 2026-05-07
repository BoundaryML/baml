package crosslang

type pointCL struct {
	x int
	y int
}

func ClassAndLoop() int {
	s := 0
	for i := 0; i < 1000; i++ {
		p := pointCL{x: i, y: i * 2}
		s += p.x + p.y
	}
	return s
}
