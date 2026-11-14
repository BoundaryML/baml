package main

import "slices"

func lcgValues(n int, seed0 int) []int {
	seed := seed0
	out := []int{}
	for i := 0; i < n; i++ {
		seed = seed * 16807 % 2147483647
		out = append(out, seed%100000)
	}
	return out
}

func mergeSort(values []int) []int {
	a := []int{}
	b := []int{}
	for _, value := range values {
		a = append(a, value)
		b = append(b, 0)
	}
	width := 1
	n := len(a)
	for width < n {
		lo := 0
		for lo < n {
			mid := lo + width
			if mid > n {
				mid = n
			}
			hi := lo + 2*width
			if hi > n {
				hi = n
			}
			i := lo
			j := mid
			k := lo
			for k < hi {
				if i < mid && (j >= hi || a[i] <= a[j]) {
					b[k] = a[i]
					i++
				} else {
					b[k] = a[j]
					j++
				}
				k++
			}
			lo += 2 * width
		}
		a, b = b, a
		width *= 2
	}
	return a
}

func checksum(xs []int) int {
	h := 0
	for i := 0; i < len(xs); i++ {
		if i > 0 && xs[i-1] > xs[i] {
			return -1
		}
		h = (h*31 + xs[i]) % 1000000007
	}
	return h
}

func Main() int {
	random := lcgValues(20000, 7)
	sorted := mergeSort(random)
	reversed := slices.Clone(sorted)
	slices.Reverse(reversed)
	dups := []int{}
	for _, v := range random {
		dups = append(dups, v%16)
	}
	total := 0
	for _, input := range [][]int{random, sorted, reversed, dups} {
		total = (total + checksum(mergeSort(input))) % 1000000007
	}
	return total
}
