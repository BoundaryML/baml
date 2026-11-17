package main

import "fmt"

func Main() int {
	names := []string{"apple", "pear", "plum", "fig", "kiwi"}
	families := []string{"FR", "VG", "DY"}
	seed := 7
	total := 0
	for i := 0; i < 10000; i++ {
		seed = seed * 16807 % 2147483647
		a := names[seed%5]
		b := families[i%3]
		s := fmt.Sprintf("%s/%s", a, b)
		total += len(s)
	}
	return total
}
