class Square:
    __slots__ = ('side',)

    def __init__(self, side):
        self.side = side

    def area(self):
        return self.side * self.side

class Rect:
    __slots__ = ('w', 'h')

    def __init__(self, w, h):
        self.w = w
        self.h = h

    def area(self):
        return self.w * self.h

def pick(i):
    return Square(i) if i % 2 == 0 else Rect(i, 3)

def main():
    acc = 0
    for i in range(100000):
        acc += pick(i).area()
    return acc
