package main

import (
	"encoding/json"
	"slices"
)

// Generate and sort (from the native-bench generate_sort case): 8,192 numbers
// from a multiplicative generator, sorted with the library sort and
// serialized to JSON.
func Main() int {
	a := []int{}
	value := 42
	for i := 0; i < 8192; i++ {
		value = value * 48271 % 1000000007
		a = append(a, value)
	}
	slices.Sort(a)
	encoded, err := json.Marshal(a)
	if err != nil {
		panic(err)
	}
	out := string(encoded)
	h := 0
	for _, c := range out {
		h = (h*31 + int(c)) % 1000000007
	}
	return h + len(out)
}
