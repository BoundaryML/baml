package main

import (
	"fmt"
)

func Main() int {
	names := []string{"apple", "pear", "plum", "fig", "kiwi"}
	families := []string{"FR", "VG", "DY"}
	limit := 25
	host := "api.example.com"
	seed := 7
	total := 0
	for i := 0; i < 10000; i++ {
		seed = seed * 16807 % 2147483647
		org := families[i%3]
		item := names[seed%5]
		id := seed % 100000
		page := i % 50
		s := fmt.Sprintf("GET /v1/orgs/%s/items/%s/%d?page=%d&limit=%d&sort=%s&order=%s host=%s user=%s-%d attempt=%d region=%s", org, item, id, page, limit, item, org, host, item, id, page, org)
		total += len(s)
	}
	return total
}
