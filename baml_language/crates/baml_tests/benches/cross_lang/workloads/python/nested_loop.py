def main() -> int:
    sum_ = 0
    for i in range(200):
        for j in range(200):
            sum_ += i * j
    return sum_
