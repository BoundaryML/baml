package main

type Inner struct {
	value int
}

type Middle struct {
	inner Inner
}

type Outer struct {
	middle Middle
}

func Main() int {
	s := 0
	for i := 0; i < 500000; i++ {
		o := Outer{middle: Middle{inner: Inner{value: i}}}
		s += o.middle.inner.value
	}
	return s
}
