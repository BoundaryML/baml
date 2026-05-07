package crosslang

func ArrayPush50K() int {
	arr := []int{}
	for i := 0; i < 50000; i++ {
		arr = append(arr, i)
	}
	return len(arr)
}
