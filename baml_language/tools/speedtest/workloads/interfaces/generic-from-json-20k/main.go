package main

import (
	"errors"
	"strconv"
	"strings"
)

var errDecode = errors.New("decode error")

// FromJSON is the constraint for a type whose pointer can fill itself from a
// JSON value (`any`: a string, an int, a []any, ...).
type FromJSON[T any] interface {
	*T
	fromJSON(j any) error
}

type Point struct {
	x, y int
}

func (p *Point) fromJSON(j any) error {
	arr, ok := j.([]any)
	if !ok {
		return errDecode
	}
	xs := []int{}
	for _, v := range arr {
		n, ok := v.(int)
		if !ok {
			return errDecode
		}
		xs = append(xs, n)
	}
	p.x = xs[0]
	p.y = xs[1]
	return nil
}

type Reading struct {
	sensor string
	value  int
}

func (r *Reading) fromJSON(j any) error {
	s, ok := j.(string)
	if !ok {
		return errDecode
	}
	parts := strings.Split(s, ":")
	value, err := strconv.Atoi(parts[1])
	if err != nil {
		value = 0
	}
	r.sensor = parts[0]
	r.value = value
	return nil
}

func parse[T any, PT FromJSON[T]](j any) (*T, error) {
	out := PT(new(T))
	if err := out.fromJSON(j); err != nil {
		return nil, err
	}
	return (*T)(out), nil
}

func Main() int {
	seed := 3
	acc := 0
	for i := 0; i < 10000; i++ {
		seed = seed * 16807 % 2147483647
		p, err := parse[Point]([]any{seed % 1000, seed % 777})
		if err != nil {
			panic(err)
		}
		r, err := parse[Reading]("s" + strconv.Itoa(seed%10) + ":" + strconv.Itoa(seed%50))
		if err != nil {
			panic(err)
		}
		acc += p.x*3 + p.y + r.value + len(r.sensor)
	}
	return acc
}
