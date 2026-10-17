def validate(x):
    if x < 0:
        return -1
    if x > 1000000:
        return -2
    if x % 7 == 0:
        return x * 2
    if x % 3 == 0:
        return x + 1
    return x

def main():
    s = 0
    for i in range(1000000):
        s += validate(i)
    return s
