package main

func Main() int {
	arr := []int{}
	for i := 0; i < 100000; i++ {
		arr = append(arr, i)
	}
	s := 0
	for _, x := range arr {
		s += x
	}
	return s
}
