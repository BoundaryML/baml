package main

import (
	"fmt"
	"strconv"
)

type Money struct {
	cents int
}

func (m *Money) String() string {
	frac := m.cents % 100
	pad := ""
	if frac < 10 {
		pad = "0"
	}
	return "$" + strconv.Itoa(m.cents/100) + "." + pad + strconv.Itoa(frac)
}

type Sku struct {
	family string
	number int
}

func (s *Sku) String() string {
	return s.family + "-" + strconv.Itoa(s.number)
}

type Item struct {
	sku   *Sku
	name  string
	price *Money
	qty   int
}

func line(item *Item) string {
	return fmt.Sprintf("%d x %s [%s] @ %s", item.qty, item.name, item.sku, item.price)
}

func Main() int {
	names := []string{"apple", "pear", "plum", "fig", "kiwi"}
	families := []string{"FR", "VG", "DY"}
	seed := 7
	total := 0
	for i := 0; i < 10000; i++ {
		seed = seed * 16807 % 2147483647
		item := &Item{
			sku:   &Sku{family: families[seed%3], number: seed % 1000},
			name:  names[i%5],
			price: &Money{cents: seed % 100000},
			qty:   seed%9 + 1,
		}
		total += len(line(item))
	}
	return total
}
