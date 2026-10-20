import threading

def f(out, idx):
    out[idx] = 1
out = [0] * 10000

def main():
    s = 0
    for i in range(10000):
        t = threading.Thread(target=f, args=(out, i))
        t.start()
        t.join()
        s += out[i]
    return s
