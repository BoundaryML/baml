import threading

def write_n(arr, n):
    for i in range(n):
        arr.append(i)

def read_n(arr, n, out, idx):
    acc = 0
    for _ in range(n):
        if len(arr) > 0:
            acc += arr[0]
    out[idx] = acc
arr = [0]
out = [0] * 4

def main():
    w = threading.Thread(target=write_n, args=(arr, 50000))
    rs = [threading.Thread(target=read_n, args=(arr, 12500, out, i)) for i in range(4)]
    w.start()
    for r in rs:
        r.start()
    w.join()
    for r in rs:
        r.join()
    return len(arr)
