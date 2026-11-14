package main

import "strings"

func Main() int {
	s := "the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow"
	delim := "|"
	count := 0
	for i := 0; i < 10000; i++ {
		parts := strings.Split(s, delim)
		count += len(parts)
	}
	return count
}
