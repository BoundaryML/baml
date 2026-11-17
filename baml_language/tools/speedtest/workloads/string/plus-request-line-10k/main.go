package main

import (
	"strconv"
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
		s := "GET /v1/orgs/" + org + "/items/" + item + "/" + strconv.Itoa(id) + "?page=" + strconv.Itoa(page) + "&limit=" + strconv.Itoa(limit) + "&sort=" + item + "&order=" + org + " host=" + host + " user=" + item + "-" + strconv.Itoa(id) + " attempt=" + strconv.Itoa(page) + " region=" + org
		total += len(s)
	}
	return total
}
