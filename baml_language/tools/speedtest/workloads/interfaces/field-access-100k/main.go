package main

// Named is an interface with one field; Go interfaces carry no fields, so the
// field read is an accessor method dispatched through the interface.
type Named interface {
	Name() string
}

type Dog struct {
	name string
	age  int
}

func (d *Dog) Name() string { return d.name }

type Robot struct {
	serial int
	model  string
}

func (r *Robot) Name() string { return r.model }

func pick(i int) Named {
	if i%2 == 0 {
		return &Dog{name: "rex", age: i}
	}
	return &Robot{serial: i, model: "r2"}
}

func nameLen(n Named) int {
	return len(n.Name())
}

func Main() int {
	acc := 0
	for i := 0; i < 100000; i++ {
		acc += nameLen(pick(i))
	}
	return acc
}
