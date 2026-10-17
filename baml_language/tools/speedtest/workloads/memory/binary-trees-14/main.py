import sys

class TreeNode:
    __slots__ = ('left', 'right')

    def __init__(self, left, right):
        self.left = left
        self.right = right

def bottom_up(d):
    if d <= 0:
        return TreeNode(None, None)
    return TreeNode(bottom_up(d - 1), bottom_up(d - 1))

def check(n):
    if n.left is None or n.right is None:
        return 1
    return 1 + check(n.left) + check(n.right)

def main():
    sys.setrecursionlimit(10000)
    max_depth = 14
    total = check(bottom_up(max_depth + 1))
    long_lived = bottom_up(max_depth)
    for depth in range(4, max_depth + 1, 2):
        iterations = 1 << max_depth - depth + 4
        total += sum((check(bottom_up(depth)) for _ in range(iterations)))
    return total + check(long_lived)
