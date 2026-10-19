class Vec:
    __slots__ = ('x', 'y')

    def __init__(self, x, y):
        self.x = x
        self.y = y

    def norm2(self):
        return self.x * self.x + self.y * self.y

def main():
    acc = 0
    for i in range(100000):
        v = Vec(i, i + 1)
        acc += v.norm2()
    return acc
