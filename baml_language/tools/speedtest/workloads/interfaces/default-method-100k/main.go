package main

// Counter has a required field (`base`, read through an accessor) and a
// required method; `total` is the interface's default method.
type Counter interface {
	getBase() int
	step() int
}

func total(c Counter) int {
	return c.getBase() + c.step()
}

type Inc struct {
	base int
}

func (c *Inc) getBase() int { return c.base }
func (c *Inc) step() int    { return 1 }

type Dbl struct {
	base int
}

func (c *Dbl) getBase() int { return c.base }
func (c *Dbl) step() int    { return c.base }

func pick(i int) Counter {
	if i%2 == 0 {
		return &Inc{base: i}
	}
	return &Dbl{base: i}
}

func run(c Counter) int {
	return total(c)
}

func Main() int {
	acc := 0
	for i := 0; i < 100000; i++ {
		acc += run(pick(i))
	}
	return acc
}
