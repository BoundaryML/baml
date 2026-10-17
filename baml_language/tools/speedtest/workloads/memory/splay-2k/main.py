class Payload:
    __slots__ = ('left', 'right', 'array', 'text')

    def __init__(self, left, right, array, text):
        self.left = left
        self.right = right
        self.array = array
        self.text = text

class Node:
    __slots__ = ('key', 'value', 'left', 'right')

    def __init__(self, key, value):
        self.key = key
        self.value = value
        self.left = None
        self.right = None

def payload(depth, tag):
    if depth == 0:
        return Payload(None, None, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 'String for key ' + str(tag) + ' in leaf node')
    return Payload(payload(depth - 1, tag), payload(depth - 1, tag), [], '')

class Tree:

    def __init__(self):
        self.root = None
        self.seed = 49734321

    def splay(self, key):
        if self.root is None:
            return
        dummy = Node(0, Payload(None, None, [], ''))
        left = right = dummy
        current = self.root
        while True:
            if key < current.key:
                if current.left is None:
                    break
                if key < current.left.key:
                    tmp = current.left
                    current.left = tmp.right
                    tmp.right = current
                    current = tmp
                    if current.left is None:
                        break
                right.left = current
                right = current
                current = current.left
            elif key > current.key:
                if current.right is None:
                    break
                if key > current.right.key:
                    tmp = current.right
                    current.right = tmp.left
                    tmp.left = current
                    current = tmp
                    if current.right is None:
                        break
                left.right = current
                left = current
                current = current.right
            else:
                break
        left.right = current.left
        right.left = current.right
        current.left = dummy.right
        current.right = dummy.left
        self.root = current

    def insert(self, key, value):
        if self.root is None:
            self.root = Node(key, value)
            return True
        self.splay(key)
        r = self.root
        if r.key == key:
            return False
        node = Node(key, value)
        if key > r.key:
            node.left = r
            node.right = r.right
            r.right = None
        else:
            node.right = r
            node.left = r.left
            r.left = None
        self.root = node
        return True

    def remove(self, key):
        self.splay(key)
        r = self.root
        if r is None or r.key != key:
            return
        if r.left is None:
            self.root = r.right
        else:
            rest = r.right
            self.root = r.left
            self.splay(key)
            self.root.right = rest

    def greatest_less_than(self, key):
        self.splay(key)
        r = self.root
        if r is None:
            return -1
        if r.key < key:
            return r.key
        cur = r.left
        if cur is None:
            return -1
        while cur.right is not None:
            cur = cur.right
        return cur.key

    def next_key(self):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % 1000000

    def insert_new(self):
        while True:
            key = self.next_key()
            if self.insert(key, payload(4, key)):
                return key

def main():
    tree = Tree()
    for _ in range(2000):
        tree.insert_new()
    for _ in range(4000):
        key = tree.insert_new()
        g = tree.greatest_less_than(key)
        tree.remove(key if g < 0 else g)
    total = 0
    count = 0
    stack = [tree.root] if tree.root is not None else []
    while stack:
        n = stack.pop()
        count += 1
        total = (total + n.key * 7 + len(n.value.text)) % 1000000007
        if n.left is not None:
            stack.append(n.left)
        if n.right is not None:
            stack.append(n.right)
    return total * 10000 + count
