def main():
    grid = []
    for i in range(1000):
        row = []
        for j in range(100):
            row.append(i * 100 + j)
        grid.append(row)
    s = 0
    for row in grid:
        for val in row:
            s += val
    return s
