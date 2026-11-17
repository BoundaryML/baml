package main

import "strconv"

type Cell struct {
	value   int
	payload []int
}

func Main() string {
	batches := 128
	width := 2048
	retain := 4
	retained := [][]*Cell{}
	checksum := 0
	for batch := 0; batch < batches; batch++ {
		cells := []*Cell{}
		for i := 0; i < width; i++ {
			cell := &Cell{value: batch + i, payload: []int{i, i + 1, i + 2, i + 3}}
			alias := cell
			alias.value++
			alias.payload[0]++
			cells = append(cells, cell)
		}
		for _, cell := range cells {
			checksum += cell.value + cell.payload[0]
		}
		if len(retained) < retain {
			retained = append(retained, cells)
		} else {
			retained[batch%retain] = cells
		}
	}
	liveSum := 0
	for _, cells := range retained {
		for _, cell := range cells {
			liveSum += cell.value + cell.payload[0]
		}
	}
	return strconv.Itoa(checksum) + "," + strconv.Itoa(liveSum)
}
