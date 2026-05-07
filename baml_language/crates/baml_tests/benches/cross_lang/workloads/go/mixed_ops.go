package crosslang

type pointMixed struct {
	x int
	y int
}

func MixedOps() int {
	sum := 0
	s := ""
	arr := []int{}
	for i := 0; i < 5000; i++ {
		sum += i*3 - 1
		s = s + "x"
		arr = append(arr, i)
		p := pointMixed{x: i, y: i + 1}
		sum += p.x + p.y
		if i > 2500 {
			sum += 1
		}
	}
	return sum + len(s) + len(arr)
}
