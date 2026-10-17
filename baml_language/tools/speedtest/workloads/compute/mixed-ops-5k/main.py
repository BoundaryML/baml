class Point:
    __slots__ = ('x', 'y')

    def __init__(self, x, y):
        self.x = x
        self.y = y

def main():
    sum = 0
    s = ''
    arr = []
    for i in range(5000):
        sum += i * 3 - 1
        s = s + 'x'
        arr.append(i)
        p = Point(i, i + 1)
        sum += p.x + p.y
        if i > 2500:
            sum += 1
    return sum + len(s) + len(arr)
