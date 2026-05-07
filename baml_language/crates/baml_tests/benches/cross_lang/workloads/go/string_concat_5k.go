package crosslang

func StringConcat5K() int {
	s := ""
	for i := 0; i < 5000; i++ {
		s = s + "hello"
	}
	return len(s)
}
