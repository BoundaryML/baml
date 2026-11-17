package main

import "strconv"

type Payload struct {
	left, right *Payload
	array       []int
	text        string
}

type SplayNode struct {
	key         int
	value       *Payload
	left, right *SplayNode
}

type SplayTree struct {
	top  *SplayNode
	seed int
}

func payload(depth, tag int) *Payload {
	if depth == 0 {
		return &Payload{array: []int{0, 1, 2, 3, 4, 5, 6, 7, 8, 9}, text: "String for key " + strconv.Itoa(tag) + " in leaf node"}
	}
	return &Payload{left: payload(depth-1, tag), right: payload(depth-1, tag), array: []int{}, text: ""}
}

func (tree *SplayTree) splay(key int) {
	current := tree.top
	if current == nil {
		return
	}
	dummy := &SplayNode{key: 0, value: &Payload{array: []int{}, text: ""}}
	left := dummy
	right := dummy
	for {
		if key < current.key {
			cl := current.left
			if cl == nil {
				break
			}
			if key < cl.key {
				current.left = cl.right
				cl.right = current
				current = cl
				if current.left == nil {
					break
				}
			}
			right.left = current
			right = current
			next := current.left
			if next == nil {
				break
			}
			current = next
		} else if key > current.key {
			cr := current.right
			if cr == nil {
				break
			}
			if key > cr.key {
				current.right = cr.left
				cr.left = current
				current = cr
				if current.right == nil {
					break
				}
			}
			left.right = current
			left = current
			next := current.right
			if next == nil {
				break
			}
			current = next
		} else {
			break
		}
	}
	left.right = current.left
	right.left = current.right
	current.left = dummy.right
	current.right = dummy.left
	tree.top = current
}

func (tree *SplayTree) insert(key int, value *Payload) bool {
	if tree.top == nil {
		tree.top = &SplayNode{key: key, value: value}
		return true
	}
	tree.splay(key)
	r := tree.top
	if r == nil || r.key == key {
		return false
	}
	node := &SplayNode{key: key, value: value}
	if key > r.key {
		node.left = r
		node.right = r.right
		r.right = nil
	} else {
		node.right = r
		node.left = r.left
		r.left = nil
	}
	tree.top = node
	return true
}

func (tree *SplayTree) remove(key int) int {
	tree.splay(key)
	r := tree.top
	if r == nil || r.key != key {
		return 0
	}
	l := r.left
	if l == nil {
		tree.top = r.right
	} else {
		rest := r.right
		tree.top = l
		tree.splay(key)
		if nr := tree.top; nr != nil {
			nr.right = rest
		}
	}
	return 1
}

func (tree *SplayTree) greatestLessThan(key int) int {
	tree.splay(key)
	r := tree.top
	if r == nil {
		return -1
	}
	if r.key < key {
		return r.key
	}
	cur := r.left
	if cur == nil {
		return -1
	}
	for cur.right != nil {
		cur = cur.right
	}
	return cur.key
}

func (tree *SplayTree) nextKey() int {
	tree.seed = tree.seed * 16807 % 2147483647
	return tree.seed % 1000000
}

func (tree *SplayTree) insertNew() int {
	for {
		key := tree.nextKey()
		if tree.insert(key, payload(4, key)) {
			return key
		}
	}
}

func Main() int {
	tree := &SplayTree{seed: 49734321}
	for i := 0; i < 2000; i++ {
		tree.insertNew()
	}
	for i := 0; i < 4000; i++ {
		key := tree.insertNew()
		g := tree.greatestLessThan(key)
		if g < 0 {
			tree.remove(key)
		} else {
			tree.remove(g)
		}
	}
	sum := 0
	count := 0
	stack := []*SplayNode{}
	if tree.top != nil {
		stack = append(stack, tree.top)
	}
	for len(stack) > 0 {
		n := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		count++
		sum = (sum + n.key*7 + len(n.value.text)) % 1000000007
		if n.left != nil {
			stack = append(stack, n.left)
		}
		if n.right != nil {
			stack = append(stack, n.right)
		}
	}
	return sum*10000 + count
}
