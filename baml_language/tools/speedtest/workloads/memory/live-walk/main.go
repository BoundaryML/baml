package main

type TNode struct {
	value    int
	parent   *TNode
	children []*TNode
}

func Setup() *TNode {
	nodes := []*TNode{}
	for i := 0; i < 262144; i++ {
		var parent *TNode
		if i > 0 {
			parent = nodes[(i-1)/2]
		}
		node := &TNode{value: (i * 13) % 1000, parent: parent, children: []*TNode{}}
		if parent != nil {
			parent.children = append(parent.children, node)
		}
		nodes = append(nodes, node)
	}
	return nodes[0]
}

func sum(node *TNode) int {
	total := node.value
	for _, child := range node.children {
		total += sum(child)
	}
	return total
}

func Run(tree *TNode) int {
	return sum(tree)
}

func Main() int {
	return Run(Setup())
}
