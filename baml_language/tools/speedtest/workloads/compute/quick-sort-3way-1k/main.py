import json


def setup():
    seed = 7
    random, dups = [], []
    for _ in range(1024):
        seed = seed * 16807 % 2147483647
        random.append(seed % 100000)
        dups.append(seed % 16)
    return [random, [i * 3 for i in range(1024)], [(1023 - i) * 3 for i in range(1024)], dups]


def quick_sort(values):
    a = list(values)
    lows, highs, top = [0], [len(a) - 1], 1
    while top > 0:
        top -= 1
        lo, hi = lows[top], highs[top]
        if lo < hi:
            pivot = a[lo + (hi - lo) // 2]
            lt, i, gt = lo, lo, hi
            while i <= gt:
                if a[i] < pivot:
                    a[lt], a[i] = a[i], a[lt]
                    lt += 1
                    i += 1
                elif a[i] > pivot:
                    a[gt], a[i] = a[i], a[gt]
                    gt -= 1
                else:
                    i += 1
            if top == len(lows):
                lows.append(lo)
                highs.append(lt - 1)
            else:
                lows[top], highs[top] = lo, lt - 1
            top += 1
            if top == len(lows):
                lows.append(gt + 1)
                highs.append(hi)
            else:
                lows[top], highs[top] = gt + 1, hi
            top += 1
    return a


def run(inputs):
    check = 0
    for values in inputs:
        out = json.dumps(quick_sort(values), separators=(",", ":"))
        h = 0
        for ch in out:
            h = (h * 31 + ord(ch)) % 1000000007
        check = (check + h + len(out)) % 1000000007
    return check


def main():
    return run(setup())
