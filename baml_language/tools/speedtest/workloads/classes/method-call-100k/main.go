package main

type Vec struct {
	x, y int
}

func (v *Vec) norm2() int {
	return v.x*v.x + v.y*v.y
}

func Main() int {
	acc := 0
	for i := 0; i < 100000; i++ {
		v := &Vec{x: i, y: i + 1}
		acc += v.norm2()
	}
	return acc
}
