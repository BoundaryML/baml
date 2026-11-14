package main

type Point struct {
	x, y int
}

func Main() int {
	sum := 0
	s := ""
	arr := []int{}
	for i := 0; i < 5000; i++ {
		sum += i*3 - 1
		s = s + "x"
		arr = append(arr, i)
		p := Point{x: i, y: i + 1}
		sum += p.x + p.y
		if i > 2500 {
			sum += 1
		}
	}
	return sum + len(s) + len(arr)
}
