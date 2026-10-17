class TNode:
    __slots__ = ("value", "parent", "children")

    def __init__(self, value, parent):
        self.value = value
        self.parent = parent
        self.children = []


def setup():
    nodes = []
    for i in range(262144):
        parent = nodes[(i - 1) // 2] if i > 0 else None
        node = TNode((i * 13) % 1000, parent)
        if parent is not None:
            parent.children.append(node)
        nodes.append(node)
    return nodes[0]


def tree_sum(node):
    total = node.value
    for child in node.children:
        total += tree_sum(child)
    return total


def run(tree):
    return tree_sum(tree)


def main():
    return run(setup())
