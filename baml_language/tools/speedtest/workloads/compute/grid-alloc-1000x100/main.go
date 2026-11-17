package main

func Main() int {
	grid := [][]int{}
	for i := 0; i < 1000; i++ {
		row := []int{}
		for j := 0; j < 100; j++ {
			row = append(row, i*100+j)
		}
		grid = append(grid, row)
	}
	s := 0
	for _, row := range grid {
		for _, val := range row {
			s += val
		}
	}
	return s
}
