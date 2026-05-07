package crosslang

func buildArray() []int {
	arr := []int{}
	for i := 0; i < 10000; i++ {
		arr = append(arr, i)
	}
	return arr
}

func ArrayIter10K() int {
	arr := buildArray()
	s := 0
	for _, x := range arr {
		s += x
	}
	return s
}
