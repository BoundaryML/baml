# Bug: deserialize for parsing an object with just a single field (should work)
"""
Run this script to see how the BAML client can be used in Python.

python -m chatbot_example
"""
import asyncio
from .pipeline import convo_demo

if __name__ == "__main__":
    asyncio.run(convo_demo())
