# Deep Dive: Swift Error Handling

## Core Philosophy: Explicit Control Flow
Swift uses a unique model that *looks* like exception handling (`throw`, `try`, `catch`) but behaves more like explicit error passing. It rejects the "unchecked exception" model of C++/Java/C# where anything can throw anywhere. In Swift, throwing is part of the function signature.

## Developer Experience (DX)

### Defining Errors

In Swift, errors are typically defined as enums conforming to the `Error` protocol:

```swift
enum FileError: Error {
    case notFound
    case permissionDenied
    case corruptedData(reason: String)
}

enum NetworkError: Error {
    case noConnection
    case timeout
    case invalidResponse(code: Int)
}
```

**Note**: `Error` is just a marker protocol (like an interface with no methods). Any type can conform to it, but enums are most common.

### Throwing Functions

Functions that can fail must be marked with `throws` in their signature:

```swift
func loadFile(at path: String) throws -> String {
    guard fileExists(path) else {
        throw FileError.notFound
    }
    guard hasPermission(path) else {
        throw FileError.permissionDenied
    }
    // Load and return file contents
    return contents
}
```

**Key Point**: Unlike Java, you don't specify *which* errors are thrown in the signature (historically). The signature just says `throws`.

### The `try` Keyword: Explicit Call Sites

Swift forces you to mark every call site that can throw with `try`. This makes control flow jumps visible during code review.

```swift
func processUserFile(username: String) {
    // This won't compile - missing 'try'
    // let data = loadFile(at: "/users/\(username)/data.txt")
    
    // You must use 'try' to acknowledge this can fail
    do {
        let data = try loadFile(at: "/users/\(username)/data.txt")
        print("Loaded: \(data)")
    } catch FileError.notFound {
        print("File doesn't exist")
    } catch FileError.permissionDenied {
        print("Access denied")
    } catch {
        // Catch-all for any other Error
        print("Unexpected error: \(error)")
    }
}
```

**Comparison to other languages**:
- **Java**: No marking at call sites; exceptions can surprise you
- **Go**: Explicit but verbose (`if err != nil` everywhere)
- **Swift**: Middle ground - explicit at call sites (`try`) but concise

### Complete Example: Multiple Operations

Here's a more complete example showing error propagation:

```swift
enum ValidationError: Error {
    case emptyUsername
    case invalidEmail
    case passwordTooShort
}

struct User {
    let username: String
    let email: String
    let password: String
}

func validateUsername(_ username: String) throws {
    if username.isEmpty {
        throw ValidationError.emptyUsername
    }
}

func validateEmail(_ email: String) throws {
    if !email.contains("@") {
        throw ValidationError.invalidEmail
    }
}

func validatePassword(_ password: String) throws {
    if password.count < 8 {
        throw ValidationError.passwordTooShort
    }
}

func createUser(username: String, email: String, password: String) throws -> User {
    // Each 'try' can potentially exit early
    try validateUsername(username)
    try validateEmail(email)
    try validatePassword(password)
    
    return User(username: username, email: email, password: password)
}

// Using it:
do {
    let user = try createUser(username: "alice", email: "alice@example.com", password: "secure123")
    print("User created: \(user.username)")
} catch ValidationError.emptyUsername {
    print("Username cannot be empty")
} catch ValidationError.invalidEmail {
    print("Email must contain @")
} catch ValidationError.passwordTooShort {
    print("Password must be at least 8 characters")
} catch {
    print("Unknown error: \(error)")
}
```

### Ergonomic Variants

Swift offers powerful sugar to convert errors into values or assertions:

#### 1. `try?` (Error to Optional)

Converts any error into `nil`. Great for "I don't care why it failed, just give me nil".

```swift
// Without try?:
let file: FileHandle?
do {
    file = try FileHandle(forReadingFrom: url)
} catch {
    file = nil
}

// With try?: much more concise
let file = try? FileHandle(forReadingFrom: url)
// file is of type FileHandle? (Optional)
// If successful: file contains the FileHandle
// If any error: file is nil
```

**Common Pattern: Guard with try?**

```swift
func processFile(at url: URL) -> String? {
    guard let fileHandle = try? FileHandle(forReadingFrom: url) else {
        return nil // File missing or unreadable - don't care which
    }
    
    guard let data = try? fileHandle.readToEnd() else {
        return nil // Read failed - don't care why
    }
    
    return String(data: data, encoding: .utf8)
}
```

**When to use**: Configuration files, optional features, fallback scenarios where the error doesn't matter.

#### 2. `try!` (Error to Crash)

Asserts that the operation will succeed. If it throws, the program crashes.

```swift
// This pattern is known to be valid, so we assert success
let emailRegex = try! NSRegularExpression(pattern: "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}")

// Loading embedded resources that MUST exist
let configPath = Bundle.main.path(forResource: "config", ofType: "json")!
let configData = try! Data(contentsOf: URL(fileURLWithPath: configPath))
```

**Warning**: `try!` is like unwrapping an optional with `!`. If it fails, your app crashes. Only use when failure is truly impossible or indicates a programmer error.

**When to use**: 
- Compile-time constants (regex patterns known to be valid)
- Bundled resources that must exist
- Situations where failure means the app is fundamentally broken

#### 3. Comparison: `try` vs `try?` vs `try!`

```swift
enum DataError: Error {
    case invalid
}

func parseData(_ input: String) throws -> Int {
    guard let value = Int(input) else {
        throw DataError.invalid
    }
    return value
}

// Standard try: Handle errors explicitly
do {
    let value = try parseData("42")
    print("Parsed: \(value)")
} catch {
    print("Failed: \(error)")
}

// try?: Convert to Optional, ignore error details
if let value = try? parseData("not a number") {
    print("Parsed: \(value)")
} else {
    print("Failed to parse") // Don't know why
}

// try!: Crash if it fails (use sparingly!)
let hardcoded = try! parseData("100") // We KNOW this is valid
```

#### 4. Propagating Errors with `try`

When a function is marked `throws`, you can propagate errors up by using `try` without `do-catch`:

```swift
func loadUserData(userId: String) throws -> User {
    // All these 'try' calls can throw, and we just propagate the error up
    let rawData = try fetchFromNetwork(userId: userId)
    let parsed = try parseJSON(rawData)
    let validated = try validateUserData(parsed)
    return validated
}

// The caller must handle errors
do {
    let user = try loadUserData(userId: "123")
    print("Loaded user: \(user)")
} catch {
    print("Failed to load user: \(error)")
}
```

This is similar to Go's `return err` or Rust's `?` operator, but with explicit `try` at each call site.

### Typed Throws (Swift 6)

Historically, Swift throws were type-erased to `any Error`. Swift 6 introduces **Typed Throws** for more precise error handling.

#### Before: Type-Erased Throws (Swift 5 and earlier)

```swift
enum ParseError: Error {
    case invalidFormat
    case missingField(String)
}

// Function signature doesn't specify what errors are thrown
func parse(string: String) throws -> Int {
    guard let value = Int(string) else {
        throw ParseError.invalidFormat
    }
    return value
}

// Caller must have a catch-all or manually check error types
do {
    let value = try parse(string: "abc")
} catch let error as ParseError {
    // We know it's ParseError, but the type system doesn't guarantee it
    print("Parse error: \(error)")
} catch {
    // Required catch-all, even though we "know" it only throws ParseError
    print("Other error: \(error)")
}
```

#### After: Typed Throws (Swift 6)

```swift
// Specify the exact error type in the signature
func parse(string: String) throws(ParseError) -> Int {
    guard let value = Int(string) else {
        throw ParseError.invalidFormat
    }
    return value
}

// Now the catch can be exhaustive without a catch-all
do {
    let value = try parse(string: "abc")
} catch .invalidFormat {
    print("Invalid format")
} catch .missingField(let field) {
    print("Missing field: \(field)")
}
// No catch-all needed! The compiler knows we've covered all ParseError cases
```

**Benefits**:
- Type safety: The compiler knows exactly what can be thrown
- Exhaustive checking: Like switch statements on enums
- Better documentation: The signature tells you what errors to expect

**Tradeoff**: Reintroduces some API coupling (like Java's checked exceptions), but with better ergonomics.

### Rethrows: Polymorphic Error Handling

`rethrows` is a sophisticated feature for higher-order functions (functions that take closures as parameters).

#### The Problem

```swift
// If transform can throw, does map throw?
func map<T>(_ transform: (Element) -> T) -> [T]
```

If we mark it `throws`, it always throws (even if the closure doesn't):

```swift
let numbers = [1, 2, 3]

// This shouldn't require 'try' because the closure doesn't throw
let doubled = numbers.map { $0 * 2 } // ERROR if map is marked throws
```

#### The Solution: `rethrows`

```swift
func map<T>(_ transform: (Element) throws -> T) rethrows -> [T]
```

**Meaning**: `map` throws **only if** the closure (`transform`) throws.

#### Complete Example

```swift
extension Array {
    // rethrows: this function throws only if transform throws
    func customMap<T>(_ transform: (Element) throws -> T) rethrows -> [T] {
        var result: [T] = []
        for element in self {
            let transformed = try transform(element)
            result.append(transformed)
        }
        return result
    }
}

let numbers = [1, 2, 3, 4]

// Non-throwing closure: no 'try' needed
let doubled = numbers.customMap { $0 * 2 }
print(doubled) // [2, 4, 6, 8]

// Throwing closure: 'try' required
enum MathError: Error {
    case divisionByZero
}

func safeDivide(_ numerator: Int, by denominator: Int) throws -> Int {
    guard denominator != 0 else {
        throw MathError.divisionByZero
    }
    return numerator / denominator
}

do {
    let results = try numbers.customMap { try safeDivide($0, by: 2) }
    print(results) // [0, 1, 1, 2]
} catch {
    print("Division failed: \(error)")
}
```

**Key Insight**: `rethrows` makes Swift's standard library functions like `map`, `filter`, `reduce` work seamlessly with both throwing and non-throwing closures. This is impossible to express in most other languages without function overloads or separate methods.

## Implementation Tradeoffs

### 1. Error Register vs. Stack Unwinding
**Tradeoff**: Swift does **not** use expensive table-based stack unwinding (like C++).

- **Implementation**: It passes errors in a dedicated CPU register (or stack slot). It's effectively a hidden return value.
- **Benefit**: Zero-cost setup (entering a `do` block is free). Throwing is very fast (comparable to returning).
- **Cost**: It's not a full stack trace. Swift errors are values, not stack captures. You don't get a traceback unless you manually capture it.

### 2. Checked vs. Unchecked
**Tradeoff**: Swift is "Checked" (must mark `try`), but historically "Unchecked Types" (throws `any Error`).

- **Benefit**: API Evolution. You can add new error cases without breaking callers (who catch generic `Error`).
- **Cost**: Callers often just `print(error)` because they don't know what specific errors to handle. Typed throws fixes this but reintroduces API coupling.

### 3. Rethrows
**Tradeoff**: Higher-order functions.

- **Problem**: `map` takes a closure. If the closure throws, does `map` throw?
- **Solution**: `rethrows`.
    ```swift
    func map<T>(_ transform: (Element) throws -> T) rethrows -> [T]
    ```
    `map` throws *only if* the closure throws. This is a sophisticated type system feature that avoids "exception swallowing" or "double wrapping".

## Summary
Swift provides the **safety of checked exceptions** without the **verbosity of Java**. It treats errors as values (Enums) but manages propagation via control flow syntax (`try`), offering a "best of both worlds" DX.
