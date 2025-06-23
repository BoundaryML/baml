#!/usr/bin/env python3

from multiprocessing import Process

def run():
    # Import and initialize BAML client inside the function
    from baml_client import b
    b.ExtractResume("test")

if __name__ == "__main__":
    # Set multiprocessing start method to 'spawn' to match Linux behavior
    import multiprocessing
    multiprocessing.set_start_method('spawn')
    
    current = Process(target=run)
    current.start()
    current.join()  3