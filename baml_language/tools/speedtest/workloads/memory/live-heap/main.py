class Item:
    __slots__ = ("id", "value", "next", "tags")

    def __init__(self, id, value, next, tags):
        self.id = id
        self.value = value
        self.next = next
        self.tags = tags


def setup():
    items = []
    for i in range(500000):
        parent = items[(i - 1) // 2] if i > 0 else None
        items.append(Item(i, (i * 7 + 12345) % 1000, parent, [i % 3]))
    return items


def run(items):
    n = len(items)
    checksum = 0
    state = 12345
    for k in range(65536):
        state = state * 48271 % 2147483647
        x = state % n
        old = items[x]
        fresh = Item(old.id, old.value, old.next, [old.id % 3])
        items[x] = fresh
        cursor = Item(k, x, fresh, [k, x])
        hops = 0
        while cursor is not None and hops < 4:
            checksum = (checksum + cursor.value * (hops + 1)) % 1000000007
            cursor = cursor.next
            hops += 1
    return f"{n},{checksum}"


def main():
    return run(setup())
