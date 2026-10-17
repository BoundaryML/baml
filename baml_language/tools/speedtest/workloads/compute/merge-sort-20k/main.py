def lcg_values(n, seed):
    out = []
    for _ in range(n):
        seed = seed * 16807 % 2147483647
        out.append(seed % 100000)
    return out

def merge_sort(values):
    a = list(values)
    b = [0] * len(values)
    width = 1
    n = len(a)
    while width < n:
        lo = 0
        while lo < n:
            mid = min(lo + width, n)
            hi = min(lo + 2 * width, n)
            i = lo
            j = mid
            k = lo
            while k < hi:
                if i < mid and (j >= hi or a[i] <= a[j]):
                    b[k] = a[i]
                    i += 1
                else:
                    b[k] = a[j]
                    j += 1
                k += 1
            lo += 2 * width
        a, b = (b, a)
        width *= 2
    return a

def checksum(xs):
    h = 0
    for i, x in enumerate(xs):
        if i > 0 and xs[i - 1] > x:
            return -1
        h = (h * 31 + x) % 1000000007
    return h

def main():
    rand = lcg_values(20000, 7)
    srt = merge_sort(rand)
    total = 0
    for inp in [rand, srt, srt[::-1], [v % 16 for v in rand]]:
        total = (total + checksum(merge_sort(inp))) % 1000000007
    return total
