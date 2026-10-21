import threading

def push_n(arr, start, n):
    for i in range(n):
        arr.append(start + i)
arr = []

def main():
    ts = [threading.Thread(target=push_n, args=(arr, i * 25000, 25000)) for i in range(4)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    return len(arr)
