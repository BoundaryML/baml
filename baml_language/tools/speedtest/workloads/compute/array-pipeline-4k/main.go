package main

import "sort"

type Item struct {
	id, weight int
}

func mapInts(xs []int, f func(int) int) []int {
	out := make([]int, 0, len(xs))
	for _, x := range xs {
		out = append(out, f(x))
	}
	return out
}

func filterInts(xs []int, f func(int) bool) []int {
	out := []int{}
	for _, x := range xs {
		if f(x) {
			out = append(out, x)
		}
	}
	return out
}

func Main() int {
	xs := []int{}
	items := []*Item{}
	seed := 7
	for i := 0; i < 4000; i++ {
		seed = seed * 16807 % 2147483647
		xs = append(xs, seed%1000)
		items = append(items, &Item{id: i, weight: seed % 97})
	}
	acc := 0
	for round := 0; round < 5; round++ {
		k := round + 2
		scaled := mapInts(xs, func(x int) int { return x*k + 1 })
		kept := filterInts(scaled, func(x int) bool { return x%3 != 0 })
		sum := 0
		for _, x := range kept {
			sum = func(a, x int) int { return a + x }(sum, x)
		}
		acc += sum
		hit := -1
		for _, x := range xs {
			if x == 990+round {
				hit = x
				break
			}
		}
		acc += hit
		heavy := []*Item{}
		for _, it := range items {
			if it.weight > 40+round {
				heavy = append(heavy, it)
			}
		}
		weights := make([]int, 0, len(heavy))
		for _, it := range heavy {
			weights = append(weights, it.weight*2+it.id%7)
		}
		total := 0
		for _, w := range weights {
			total += w + round
		}
		acc += total
		part := append([]*Item{}, items[round*100:round*100+400]...)
		sort.SliceStable(part, func(i, j int) bool { return part[i].weight < part[j].weight })
		acc += part[0].id + part[399].id*3
	}
	return acc
}
