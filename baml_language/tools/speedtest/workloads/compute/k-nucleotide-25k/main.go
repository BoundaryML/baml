package main

import "strings"

func dna(n int) string {
	seed := 42
	out := []string{}
	for i := 0; i < n; i++ {
		seed = (seed*3877 + 29573) % 139968
		if seed < 42000 {
			out = append(out, "a")
		} else if seed < 70000 {
			out = append(out, "c")
		} else if seed < 98000 {
			out = append(out, "g")
		} else {
			out = append(out, "t")
		}
	}
	return strings.Join(out, "")
}

func count(seq string, k int) map[string]int {
	counts := map[string]int{}
	last := len(seq) - k
	for i := 0; i <= last; i++ {
		counts[seq[i:i+k]]++
	}
	return counts
}

func Main() int {
	seq := dna(25000)
	check := 0
	for _, k := range []int{1, 2, 3, 4, 6, 12, 18} {
		counts := count(seq, k)
		most := 0
		for _, v := range counts {
			if v > most {
				most = v
			}
		}
		check = check*31 + len(counts)*1000 + most
		check = check % 1000000007
	}
	probe := count(seq, 3)
	return check + probe["ggt"] + count(seq, 6)["ggtatt"]
}
