package main

import "strconv"

type Vec2 struct {
	x, y int
}

func add(a, b *Vec2) *Vec2 { return &Vec2{x: a.x + b.x, y: a.y + b.y} }

func scale(a *Vec2, k int) *Vec2 {
	return &Vec2{x: (a.x * k) % 1000003, y: (a.y * k) % 1000003}
}

func pick(a, b *Vec2) *Vec2 {
	if a.x <= b.x {
		return a
	}
	return b
}

func swap(ring []*Vec2, slot int, v *Vec2) *Vec2 {
	old := ring[slot]
	ring[slot] = v
	return old
}

func Main() string {
	ring := []*Vec2{{x: 0, y: 0}, {x: 1, y: 1}, {x: 2, y: 2}, {x: 3, y: 3}}
	acc := &Vec2{x: 7, y: 1}
	step := &Vec2{x: 3, y: 5}
	for i := 0; i < 4096; i++ {
		moved := add(acc, step)
		scaled := scale(moved, 7)
		old := swap(ring, i%4, scaled)
		acc = pick(add(old, step), moved)
	}
	return strconv.Itoa(acc.x) + "," + strconv.Itoa(acc.y) + "," + strconv.Itoa(ring[0].x) + "," + strconv.Itoa(ring[3].y)
}
