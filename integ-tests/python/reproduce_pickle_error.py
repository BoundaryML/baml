#!/usr/bin/env python3
"""
Script to reproduce the BAML pickling error that occurs with multiprocessing.
"""

import pickle
import sys
import os

# Add the test project to the path so we can import baml_client
sys.path.insert(0, 'integ-tests/python/docker-tests/test-project')

try:
    from baml_client import b
    
    print("Testing BAML client pickling...")
    print(f"BAML client type: {type(b)}")
    
    # Try to pickle the client - this should fail with the current version
    print("Attempting to pickle the BAML client...")
    pickled_data = pickle.dumps(b)
    print("✅ SUCCESS: BAML client pickled successfully!")
    
    # Try to unpickle it
    print("Attempting to unpickle the BAML client...")
    b2 = pickle.loads(pickled_data)
    print("✅ SUCCESS: BAML client unpickled successfully!")
    
    # Test that the unpickled client works
    print("Testing unpickled client functionality...")
    result = b2.ExtractResume("John Doe\nSoftware Engineer\nPython, Rust")
    print(f"✅ SUCCESS: Unpickled client works! Result type: {type(result)}")
    
except Exception as e:
    print(f"❌ ERROR: {type(e).__name__}: {e}")
    import traceback
    traceback.print_exc()

print("\n" + "="*50)
print("Testing multiprocessing scenario...")

from multiprocessing import Process
import multiprocessing

def worker_function():
    """Function that will run in a separate process"""
    try:
        from baml_client import b
        result = b.ExtractResume("Jane Smith\nData Scientist\nPython, Machine Learning")
        print(f"Worker process success: {type(result)}")
        return True
    except Exception as e:
        print(f"Worker process error: {e}")
        return False

if __name__ == "__main__":
    # Set start method to 'spawn' to force pickling (like on some systems)
    try:
        multiprocessing.set_start_method('spawn', force=True)
    except RuntimeError:
        pass  # Already set
    
    print("Testing multiprocessing with spawn method...")
    process = Process(target=worker_function)
    try:
        process.start()
        process.join()
        if process.exitcode == 0:
            print("✅ SUCCESS: Multiprocessing worked!")
        else:
            print(f"❌ ERROR: Process exited with code {process.exitcode}")
    except Exception as e:
        print(f"❌ ERROR in multiprocessing: {e}")
        import traceback
        traceback.print_exc() 