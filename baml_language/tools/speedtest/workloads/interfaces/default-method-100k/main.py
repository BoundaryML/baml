class Inc:
    __slots__ = ('base',)

    def __init__(self, base):
        self.base = base

    def step(self):
        return 1

    def total(self):
        return self.base + self.step()

class Dbl:
    __slots__ = ('base',)

    def __init__(self, base):
        self.base = base

    def step(self):
        return self.base

    def total(self):
        return self.base + self.step()

def pick(i):
    return Inc(i) if i % 2 == 0 else Dbl(i)

def main():
    acc = 0
    for i in range(100000):
        acc += pick(i).total()
    return acc
