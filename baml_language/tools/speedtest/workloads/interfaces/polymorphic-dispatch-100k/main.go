package main

type Shape interface {
	area() int
}

type Square struct {
	side int
}

func (s *Square) area() int { return s.side * s.side }

type Rect struct {
	w, h int
}

func (r *Rect) area() int { return r.w * r.h }

func pick(i int) Shape {
	if i%2 == 0 {
		return &Square{side: i}
	}
	return &Rect{w: i, h: 3}
}

func areaOf(s Shape) int {
	return s.area()
}

func Main() int {
	acc := 0
	for i := 0; i < 100000; i++ {
		acc += areaOf(pick(i))
	}
	return acc
}
