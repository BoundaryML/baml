import json


def main():
    a = []
    value = 42
    for _ in range(8192):
        value = value * 48271 % 1000000007
        a.append(value)
    a.sort()
    out = json.dumps(a, separators=(",", ":"))
    h = 0
    for ch in out:
        h = (h * 31 + ord(ch)) % 1000000007
    return h + len(out)
