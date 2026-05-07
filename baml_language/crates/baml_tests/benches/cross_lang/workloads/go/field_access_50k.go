package crosslang

type pointXYZ struct {
	x int
	y int
	z int
}

func FieldAccess50K() int {
	p := pointXYZ{x: 1, y: 2, z: 3}
	s := 0
	for i := 0; i < 50000; i++ {
		_ = i
		s += p.x + p.y + p.z
	}
	return s
}
