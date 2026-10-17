class Leaf:
    __slots__ = ('a', 'b', 'c', 'd')

    def __init__(self, a, b, c, d):
        self.a = a
        self.b = b
        self.c = c
        self.d = d

class Node:
    __slots__ = ('left', 'right', 'n0', 'n1', 'n2', 'n3')

    def __init__(self, left, right, n0, n1, n2, n3):
        self.left = left
        self.right = right
        self.n0 = n0
        self.n1 = n1
        self.n2 = n2
        self.n3 = n3

class BigRecord:
    __slots__ = ('id', 'a0', 'a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 'primary', 'secondary', 'tail')

    def __init__(self, id, a0, a1, a2, a3, a4, a5, a6, a7, primary, secondary, tail):
        self.id = id
        self.a0 = a0
        self.a1 = a1
        self.a2 = a2
        self.a3 = a3
        self.a4 = a4
        self.a5 = a5
        self.a6 = a6
        self.a7 = a7
        self.primary = primary
        self.secondary = secondary
        self.tail = tail

def main():
    s = 0
    for i in range(50000):
        r = BigRecord(i, i + 1, i + 2, i + 3, i + 4, i + 5, i + 6, i + 7, i + 8, Node(Leaf(i, i + 1, i + 2, i + 3), Leaf(i + 4, i + 5, i + 6, i + 7), i + 8, i + 9, i + 10, i + 11), Node(Leaf(i + 12, i + 13, i + 14, i + 15), Leaf(i + 16, i + 17, i + 18, i + 19), i + 20, i + 21, i + 22, i + 23), Leaf(i + 24, i + 25, i + 26, i + 27))
        s += r.id + r.a7 + r.primary.left.c + r.primary.right.d + r.secondary.n3 + r.secondary.right.b + r.tail.d
    return s
