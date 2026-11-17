package main

type TreeNode struct {
	left, right *TreeNode
}

func bottomUp(depth int) *TreeNode {
	if depth <= 0 {
		return &TreeNode{}
	}
	return &TreeNode{left: bottomUp(depth - 1), right: bottomUp(depth - 1)}
}

func check(node *TreeNode) int {
	l := node.left
	r := node.right
	if l == nil || r == nil {
		return 1
	}
	return 1 + check(l) + check(r)
}

func Main() int {
	maxDepth := 14
	total := check(bottomUp(maxDepth + 1))
	longLived := bottomUp(maxDepth)
	for depth := 4; depth <= maxDepth; depth += 2 {
		iterations := 1 << (maxDepth - depth + 4)
		sum := 0
		for i := 0; i < iterations; i++ {
			sum += check(bottomUp(depth))
		}
		total += sum
	}
	return total + check(longLived)
}
