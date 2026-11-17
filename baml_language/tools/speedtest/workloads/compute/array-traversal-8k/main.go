package main

func Setup() []int {
	seed := 3
	out := []int{}
	for i := 0; i < 8192; i++ {
		seed = seed * 16807 % 2147483647
		out = append(out, seed%1000)
	}
	return out
}

func iterate(xs []int) int {
	total := 0
	for _, v := range xs {
		total += v
	}
	return total
}

func indexed(xs []int) int {
	total := 0
	i := 0
	for i < len(xs) {
		total += xs[i]
		i++
	}
	return total
}

func rebuild(xs []int) int {
	ys := []int{}
	j := 0
	for j < len(xs) {
		ys = append(ys, xs[j])
		j++
	}
	return iterate(ys)
}

func Run(xs []int) int {
	return iterate(xs) + indexed(xs)*3 + rebuild(xs)*7
}

func Main() int {
	return Run(Setup())
}
