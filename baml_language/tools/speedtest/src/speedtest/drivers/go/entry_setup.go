package main

func main() {
	state := Setup()
	measure(func() any { return Run(state) })
}
