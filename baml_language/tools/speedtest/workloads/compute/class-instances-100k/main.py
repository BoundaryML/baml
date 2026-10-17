class Point:
    __slots__ = ('x', 'y')

    def __init__(self, x, y):
        self.x = x
        self.y = y

def main():
    s = 0
    for i in range(100000):
        p = Point(i, i * 2)
        s += p.x + p.y
    return s
