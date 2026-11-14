package main

type Animal interface {
	noise() int
}

type Dog struct{}

func (d *Dog) noise() int { return 3 }

type Cat struct{}

func (c *Cat) noise() int { return 5 }

func pick(i int) Animal {
	if i%2 == 0 {
		return &Dog{}
	}
	return &Cat{}
}

func code(a Animal) int {
	switch v := a.(type) {
	case *Dog:
		return v.noise()
	case *Cat:
		return v.noise()
	default:
		return 0
	}
}

func Main() int {
	acc := 0
	for i := 0; i < 100000; i++ {
		acc += code(pick(i))
	}
	return acc
}
