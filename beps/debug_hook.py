
import sys
import os
from pathlib import Path

# Add current dir to path to import bep_hooks
sys.path.append(os.getcwd())
import bep_hooks

# Mock Page object
class MockFile:
    def __init__(self, src_path):
        self.src_path = src_path

class MockPage:
    def __init__(self, src_path):
        self.file = MockFile(src_path)

# Read the actual file content
file_path = Path("docs/proposals/BEP-001-exceptions/context/go.md")
with open(file_path, "r") as f:
    markdown = f.read()

print(f"Original Markdown Length: {len(markdown)}")

# Run the hook
try:
    result = bep_hooks.on_page_markdown(markdown, MockPage("proposals/BEP-001-exceptions/context/go.md"))
    print(f"Result Markdown Length: {len(result)}")
    print("--- RESULT START ---")
    print(result[:500]) # Print first 500 chars
    print("--- RESULT END ---")
    print("--- RESULT TAIL ---")
    print(result[-500:])
    print("--- RESULT TAIL END ---")
    
    if len(result) == 0:
        print("ERROR: Result is empty!")
except Exception as e:
    print(f"ERROR: Hook failed with {e}")
    import traceback
    traceback.print_exc()
