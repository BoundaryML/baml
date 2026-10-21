class Account:
    __slots__ = ('balance',)

    def __init__(self, balance):
        self.balance = balance

    def fee(self):
        return self.balance // 100

    def net(self):
        return self.balance - self.fee()

def main():
    acc = 0
    for i in range(100000):
        a = Account(i)
        acc += a.net()
    return acc
