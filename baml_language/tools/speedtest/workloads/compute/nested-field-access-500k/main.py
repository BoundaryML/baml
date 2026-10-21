class Inner:
    __slots__ = ('value',)

    def __init__(self, v):
        self.value = v

class Middle:
    __slots__ = ('inner',)

    def __init__(self, i):
        self.inner = i

class Outer:
    __slots__ = ('middle',)

    def __init__(self, m):
        self.middle = m

def main():
    s = 0
    for i in range(500000):
        o = Outer(Middle(Inner(i)))
        s += o.middle.inner.value
    return s
