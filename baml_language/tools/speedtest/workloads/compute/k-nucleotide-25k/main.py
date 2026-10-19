def dna(n):
    seed = 42
    out = []
    for _ in range(n):
        seed = (seed * 3877 + 29573) % 139968
        if seed < 42000:
            out.append('a')
        elif seed < 70000:
            out.append('c')
        elif seed < 98000:
            out.append('g')
        else:
            out.append('t')
    return ''.join(out)

def count(seq, k):
    counts = {}
    for i in range(len(seq) - k + 1):
        key = seq[i:i + k]
        counts[key] = counts.get(key, 0) + 1
    return counts
seq = dna(25000)

def main():
    check = 0
    for k in [1, 2, 3, 4, 6, 12, 18]:
        counts = count(seq, k)
        most = max(counts.values())
        check = (check * 31 + len(counts) * 1000 + most) % 1000000007
    return check + count(seq, 3).get('ggt', 0) + count(seq, 6).get('ggtatt', 0)
