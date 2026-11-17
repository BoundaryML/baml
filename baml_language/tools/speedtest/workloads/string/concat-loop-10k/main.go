package main

func Main() int {
	s := ""
	for i := 0; i < 10000; i++ {
		s = s + "x"
	}
	return len(s)
}
