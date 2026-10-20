class Dog:
    __slots__ = ()

    def noise(self):
        return 3

class Cat:
    __slots__ = ()

    def noise(self):
        return 5

def pick(i):
    return Dog() if i % 2 == 0 else Cat()

def code(a):
    if isinstance(a, Dog):
        return a.noise()
    elif isinstance(a, Cat):
        return a.noise()
    else:
        return 0

def main():
    acc = 0
    for i in range(100000):
        acc += code(pick(i))
    return acc
