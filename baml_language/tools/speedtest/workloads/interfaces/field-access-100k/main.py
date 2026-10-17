class Dog:
    __slots__ = ("name", "age")

    def __init__(self, name, age):
        self.name = name
        self.age = age


class Robot:
    __slots__ = ("serial", "model")

    def __init__(self, serial, model):
        self.serial = serial
        self.model = model

    @property
    def name(self):
        return self.model


def pick(i):
    return Dog("rex", i) if i % 2 == 0 else Robot(i, "r2")


def name_len(n):
    return len(n.name)


def main():
    acc = 0
    for i in range(100000):
        acc += name_len(pick(i))
    return acc
