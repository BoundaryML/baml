import threading


def main():
    lock = threading.RLock()
    acc = [0]

    def add():
        acc[0] += 2

    for _ in range(1000000):
        with lock:
            add()
    return acc[0]
