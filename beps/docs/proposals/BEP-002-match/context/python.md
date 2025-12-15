# Python Match Syntax

Introduced in Python 3.10 (PEP 634), the `match` statement provides structural pattern matching.

## Basic Syntax

```python
match command:
    case "quit":
        quit()
    case "reset":
        reset()
    case _:
        print("Unknown command")
```

## Key Features

### 1. Structural Matching

Matches against the structure of data (sequences, mappings, objects).

```python
match point:
    case (0, 0):
        print("Origin")
    case (0, y):
        print(f"Y={y}")
    case (x, 0):
        print(f"X={x}")
    case (x, y):
        print(f"X={x}, Y={y}")
```

### 2. Class Matching

Matches against class attributes.

```python
@dataclass
class Point:
    x: int
    y: int

match point:
    case Point(x=0, y=0):
        print("Origin")
    case Point(x=0, y=y):
        print(f"Y={y}")
```

### 3. Guards

`if` clauses can be added to cases.

```python
match point:
    case Point(x, y) if x == y:
        print(f"Y=X at {x}")
    case Point(x, y):
        print(f"Not on diagonal")
```

### 4. Capture Patterns

Variables can capture parts of the match.

```python
match command.split():
    case ["go", direction]:
        print(f"Going {direction}")
    case ["drop", *items]:
        print(f"Dropping {items}")
```

### 5. OR Patterns

Combine patterns with `|`.

```python
match status:
    case 401 | 403 | 404:
        print("Not allowed")
    case 200:
        print("OK")
```

### Note

Python's `match` is a **statement**, not an expression. It does not return a value.
