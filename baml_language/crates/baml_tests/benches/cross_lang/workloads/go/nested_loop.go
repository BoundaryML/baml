package crosslang

func NestedLoop() int {
	sum := 0
	for i := 0; i < 200; i++ {
		for j := 0; j < 200; j++ {
			sum += i * j
		}
	}
	return sum
}
