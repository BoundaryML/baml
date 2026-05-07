package crosslang

func Loop500K() int {
	sum := 0
	for i := 0; i < 500000; i++ {
		sum += i
	}
	return sum
}
