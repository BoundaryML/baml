package main

import "strings"

func Main() int {
	count := 0
	for i := 0; i < 100000; i++ {
		s := "hello world foo bar baz qux"
		parts := strings.Split(s, " ")
		count += len(parts)
	}
	return count
}
