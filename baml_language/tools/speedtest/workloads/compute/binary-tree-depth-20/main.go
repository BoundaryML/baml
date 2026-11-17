package main

func treeSum(depth int) int {
	if depth <= 0 {
		return 1
	}
	return treeSum(depth-1) + treeSum(depth-1)
}

func Main() int {
	return treeSum(20)
}
