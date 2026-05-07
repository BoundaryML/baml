package crosslang

func addOne(n int) int { return n + 1 }

func ClosureCall50K() int {
	f := addOne
	s := 0
	for i := 0; i < 50000; i++ {
		s += f(i)
	}
	return s
}
