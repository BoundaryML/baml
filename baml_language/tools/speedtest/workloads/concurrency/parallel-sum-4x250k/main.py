from concurrent.futures import ThreadPoolExecutor

def sum_range(start, end):
    s = 0
    for i in range(start, end):
        s += i
    return s

def main():
    with ThreadPoolExecutor(max_workers=4) as ex:
        fs = [ex.submit(sum_range, s, e) for s, e in [(0, 250000), (250000, 500000), (500000, 750000), (750000, 1000000)]]
        return sum((f.result() for f in fs))
