from multiprocessing import Process
from baml_client import b

def run():
    b.ExtractResume("test")

if __name__ == "__main__":
    current = Process(target=run)
    current.start()
    current.join()  