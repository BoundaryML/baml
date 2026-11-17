package main

import "strconv"

type Node struct {
	id     int
	value  int
	edges  []*Node
	parent *Node
}

func request(r, n int) int {
	nodes := []*Node{}
	for i := 0; i < n; i++ {
		nodes = append(nodes, &Node{id: i, value: (r*31 + i*17) % 1000, edges: []*Node{}})
	}
	for _, node := range nodes {
		node.edges = append(node.edges, nodes[(node.id+1)%n])
		node.edges = append(node.edges, nodes[(node.id*7+r)%n])
		node.parent = nodes[node.id/2]
	}
	sum := 0
	for _, node := range nodes {
		for _, edge := range node.edges {
			sum += edge.value
		}
		if parent := node.parent; parent != nil {
			sum += parent.id
		}
	}
	return sum
}

func Main() string {
	requests := 64
	nodes := 256
	checksum := 0
	for r := 0; r < requests; r++ {
		checksum = (checksum*31 + request(r, nodes)) % 1000000007
	}
	return strconv.Itoa(requests*nodes) + "," + strconv.Itoa(checksum)
}
