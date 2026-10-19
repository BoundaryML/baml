def t(d):
    if d <= 0:
        return 1
    return t(d - 1) + t(d - 1)

def main():
    return t(20)
