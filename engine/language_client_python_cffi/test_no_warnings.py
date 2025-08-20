#!/usr/bin/env python
"""Test to verify no resource warnings are raised"""

import subprocess
import sys
import os

# Set library path
os.environ['BAML_LIBRARY_PATH'] = '/Users/greghale/code/baml-4/engine/target/debug/libbaml_cffi.dylib'

# Run pytest with warnings
result = subprocess.run(
    [sys.executable, '-m', 'pytest', 'tests/test_callbacks.py', '-v', '-W', 'error'],
    capture_output=True,
    text=True
)

print("STDOUT:")
print(result.stdout)
print("\nSTDERR:")
print(result.stderr)
print(f"\nReturn code: {result.returncode}")

# Check for the specific warning we fixed
if "PytestUnraisableExceptionWarning" in result.stderr or "PytestUnraisableExceptionWarning" in result.stdout:
    print("\n❌ WARNING FOUND: PytestUnraisableExceptionWarning still present!")
    sys.exit(1)
else:
    print("\n✅ No PytestUnraisableExceptionWarning found - fix successful!")
    sys.exit(0)