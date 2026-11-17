package main

import "strings"

func Main() int {
	s := "hello world foo bar baz qux"
	delim := " "
	count := 0
	for i := 0; i < 100000; i++ {
		parts := strings.Split(s, delim)
		count += len(parts)
	}
	return count
}
