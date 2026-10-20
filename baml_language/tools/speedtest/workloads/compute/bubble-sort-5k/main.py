def main():
    arr = list(range(5000, 0, -1))
    n = len(arr)
    for i in range(n):
        for j in range(n - i - 1):
            if arr[j] > arr[j + 1]:
                arr[j], arr[j + 1] = (arr[j + 1], arr[j])
    return arr[0] + arr[4999]
