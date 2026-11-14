package main

import "encoding/json"

func Setup() [][]int {
	seed := 7
	random := []int{}
	dups := []int{}
	for i := 0; i < 1024; i++ {
		seed = seed * 16807 % 2147483647
		random = append(random, seed%100000)
		dups = append(dups, seed%16)
	}
	sorted := []int{}
	reversed := []int{}
	for i := 0; i < 1024; i++ {
		sorted = append(sorted, i*3)
		reversed = append(reversed, (1023-i)*3)
	}
	return [][]int{random, sorted, reversed, dups}
}

func quickSort(values []int) []int {
	a := []int{}
	for _, v := range values {
		a = append(a, v)
	}
	lows := []int{0}
	highs := []int{len(a) - 1}
	top := 1
	for top > 0 {
		top--
		lo := lows[top]
		hi := highs[top]
		if lo < hi {
			pivot := a[lo+(hi-lo)/2]
			lt := lo
			i := lo
			gt := hi
			for i <= gt {
				if a[i] < pivot {
					a[lt], a[i] = a[i], a[lt]
					lt++
					i++
				} else if a[i] > pivot {
					a[gt], a[i] = a[i], a[gt]
					gt--
				} else {
					i++
				}
			}
			if top == len(lows) {
				lows = append(lows, lo)
				highs = append(highs, lt-1)
			} else {
				lows[top] = lo
				highs[top] = lt - 1
			}
			top++
			if top == len(lows) {
				lows = append(lows, gt+1)
				highs = append(highs, hi)
			} else {
				lows[top] = gt + 1
				highs[top] = hi
			}
			top++
		}
	}
	return a
}

func Run(inputs [][]int) int {
	check := 0
	for _, input := range inputs {
		encoded, err := json.Marshal(quickSort(input))
		if err != nil {
			panic(err)
		}
		out := string(encoded)
		h := 0
		for _, c := range out {
			h = (h*31 + int(c)) % 1000000007
		}
		check = (check + h + len(out)) % 1000000007
	}
	return check
}

func Main() int {
	return Run(Setup())
}
