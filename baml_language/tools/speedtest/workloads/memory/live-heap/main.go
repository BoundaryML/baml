package main

import "strconv"

type Item struct {
	id    int
	value int
	next  *Item
	tags  []int
}

func Setup() []*Item {
	items := []*Item{}
	for i := 0; i < 500000; i++ {
		var parent *Item
		if i > 0 {
			parent = items[(i-1)/2]
		}
		items = append(items, &Item{id: i, value: (i*7 + 12345) % 1000, next: parent, tags: []int{i % 3}})
	}
	return items
}

func Run(items []*Item) string {
	n := len(items)
	checksum := 0
	state := 12345
	for k := 0; k < 65536; k++ {
		state = state * 48271 % 2147483647
		x := state % n
		old := items[x]
		fresh := &Item{id: old.id, value: old.value, next: old.next, tags: []int{old.id % 3}}
		items[x] = fresh
		temp := &Item{id: k, value: x, next: fresh, tags: []int{k, x}}
		cursor := temp
		hops := 0
		for cursor != nil && hops < 4 {
			checksum = (checksum + cursor.value*(hops+1)) % 1000000007
			cursor = cursor.next
			hops++
		}
	}
	return strconv.Itoa(n) + "," + strconv.Itoa(checksum)
}

func Main() string {
	return Run(Setup())
}
