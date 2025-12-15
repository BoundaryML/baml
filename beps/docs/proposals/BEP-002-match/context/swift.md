# Swift Switch/Match Syntax

Swift uses the `switch` statement for pattern matching. It is very powerful and must be exhaustive.

## Basic Syntax

```swift
let someCharacter: Character = "z"
switch someCharacter {
case "a":
    print("The first letter of the alphabet")
case "z":
    print("The last letter of the alphabet")
default:
    print("Some other character")
}
```

## Key Features

### 1. Exhaustiveness

Switches must be exhaustive. For enums, this means covering all cases.

```swift
enum CompassPoint {
    case north, south, east, west
}

var directionToHead = CompassPoint.south
switch directionToHead {
case .north:
    print("Lots of planets have a north")
case .south:
    print("Watch out for penguins")
case .east:
    print("Where the sun rises")
case .west:
    print("Where the skies are blue")
}
```

### 2. Interval Matching

```swift
let approximateCount = 62
let countedThings = "moons orbiting Saturn"
let naturalCount: String
switch approximateCount {
case 0:
    naturalCount = "no"
case 1..<5:
    naturalCount = "a few"
case 5..<12:
    naturalCount = "several"
case 12..<100:
    naturalCount = "dozens of"
default:
    naturalCount = "hundreds of"
}
```

### 3. Tuples and Value Binding

```swift
let anotherPoint = (2, 0)
switch anotherPoint {
case (let x, 0):
    print("on the x-axis with an x value of \(x)")
case (0, let y):
    print("on the y-axis with a y value of \(y)")
case let (x, y):
    print("somewhere else at (\(x), \(y))")
}
```

### 4. Where Clauses (Guards)

```swift
let yetAnotherPoint = (1, -1)
switch yetAnotherPoint {
case let (x, y) where x == y:
    print("(\(x), \(y)) is on the line x == y")
case let (x, y) where x == -y:
    print("(\(x), \(y)) is on the line x == -y")
case let (x, y):
    print("(\(x), \(y)) is just some arbitrary point")
}
```

### 5. `if case` and `guard case`

For matching a single pattern without a full switch.

```swift
if case .some(let x) = optionalValue {
    print(x)
}
```

### Note

Swift's `switch` is a **statement**, but Swift 5.9 introduced `if` and `switch` expressions that can return values.
