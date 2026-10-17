from concurrent.futures import ThreadPoolExecutor

def task(i):
    return i * 2

def main():
    with ThreadPoolExecutor(max_workers=4) as ex:
        futures = [ex.submit(task, i) for i in range(100)]
        return sum((f.result() for f in futures))
