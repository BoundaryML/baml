package main

import "strings"

func Main() int {
	s := "                              the quick brown fox jumps over the lazy dog and runs through the meadow with great enthusiasm                              "
	total := 0
	for i := 0; i < 100000; i++ {
		t := strings.TrimSpace(s)
		total += len(t)
	}
	return total
}
