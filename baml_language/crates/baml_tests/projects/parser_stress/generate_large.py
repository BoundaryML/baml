#!/usr/bin/env python3
"""Generate a large BAML file for stress testing the parser."""

def generate_large_file():
    lines = []
    
    # Generate 1000 classes
    for i in range(1, 1001):
        lines.append(f"class User{i} {{")
        lines.append(f"  name string @alias(\"user_{i}_name\")")
        lines.append(f"  email string @description(\"Email for user {i}\")")
        lines.append(f"  age int")
        lines.append(f"}}")
        lines.append("")
    
    # Generate 100 enums
    for i in range(1, 101):
        lines.append(f"enum Status{i} {{")
        lines.append(f"  ACTIVE_{i}")
        lines.append(f"  INACTIVE_{i}")
        lines.append(f"  PENDING_{i}")
        lines.append(f"}}")
        lines.append("")
    
    # Generate 100 functions
    for i in range(1, 101):
        lines.append(f"function Process{i}(input string) -> string {{")
        lines.append(f"  client GPT4")
        lines.append(f'  prompt "Process input {i}: {{{{ input }}}}"')
        lines.append(f"}}")
        lines.append("")
    
    return "\n".join(lines)

if __name__ == "__main__":
    content = generate_large_file()
    with open("large_file.baml", "w") as f:
        f.write(content)
    print(f"Generated large_file.baml with {len(content)} characters")
