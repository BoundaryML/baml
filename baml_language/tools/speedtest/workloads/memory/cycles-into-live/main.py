class Node:
    __slots__ = ("id", "value", "edges")

    def __init__(self, id, value, edges):
        self.id = id
        self.value = value
        self.edges = edges


def setup():
    live = [Node(i, (i * 7) % 1000, []) for i in range(500000)]
    for node in live:
        node.edges.append(live[(node.id + 1) % 500000])
    return live


def run(live):
    n = len(live)
    checksum = 0
    for r in range(65536):
        a = Node(r, r % 1000, [])
        b = Node(r + 1, 0, [a])
        b.edges.append(live[(r * 7919) % n])
        a.edges.append(b)
        total = 0
        for edge in b.edges:
            total += edge.value
        checksum = (checksum * 31 + total) % 1000000007
    return f"{n},{checksum}"


def main():
    return run(setup())
