package main

import "strconv"

type Node struct {
	id    int
	value int
	edges []*Node
}

func Setup() []*Node {
	live := []*Node{}
	for i := 0; i < 500000; i++ {
		live = append(live, &Node{id: i, value: (i * 7) % 1000, edges: []*Node{}})
	}
	for _, node := range live {
		node.edges = append(node.edges, live[(node.id+1)%500000])
	}
	return live
}

func Run(live []*Node) string {
	n := len(live)
	checksum := 0
	for r := 0; r < 65536; r++ {
		a := &Node{id: r, value: r % 1000, edges: []*Node{}}
		b := &Node{id: r + 1, value: 0, edges: []*Node{a}}
		b.edges = append(b.edges, live[(r*7919)%n])
		a.edges = append(a.edges, b)
		sum := 0
		for _, edge := range b.edges {
			sum += edge.value
		}
		checksum = (checksum*31 + sum) % 1000000007
	}
	return strconv.Itoa(n) + "," + strconv.Itoa(checksum)
}

func Main() string {
	return Run(Setup())
}
