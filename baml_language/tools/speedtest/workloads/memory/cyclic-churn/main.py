class Node:
    __slots__ = ("id", "value", "edges", "parent")

    def __init__(self, id, value):
        self.id = id
        self.value = value
        self.edges = []
        self.parent = None


def request(r, n):
    nodes = [Node(i, (r * 31 + i * 17) % 1000) for i in range(n)]
    for node in nodes:
        node.edges.append(nodes[(node.id + 1) % n])
        node.edges.append(nodes[(node.id * 7 + r) % n])
        node.parent = nodes[node.id // 2]
    total = 0
    for node in nodes:
        for edge in node.edges:
            total += edge.value
        if node.parent is not None:
            total += node.parent.id
    return total


def main():
    requests, nodes = 64, 256
    checksum = 0
    for r in range(requests):
        checksum = (checksum * 31 + request(r, nodes)) % 1000000007
    return f"{requests * nodes},{checksum}"
