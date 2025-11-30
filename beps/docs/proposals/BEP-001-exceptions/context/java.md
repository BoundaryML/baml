# Deep Dive: Java Error Handling

## Core Philosophy: Checked Exceptions
Java is famous (or infamous) for **Checked Exceptions**. The compiler enforces that if a method throws a checked exception, the caller *must* handle it or declare it.

## Developer Experience (DX)

### The Verbosity of Safety
```java
public void readFile() throws IOException, FileNotFoundException { ... }

public void main() {
    try {
        readFile();
    } catch (FileNotFoundException e) {
        // Handle specific
    } catch (IOException e) {
        // Handle general
    }
}
```

**DX Pros**: Self-documenting APIs. You know exactly what can go wrong.

**DX Cons**: "Catch and Ignore" is rampant. Developers get tired of bubbling up exceptions and write `catch (Exception e) { e.printStackTrace(); }`.

### The Lambda Problem
Checked exceptions clash with modern functional features (Lambdas/Streams).
```java
List<String> lines = files.stream()
    .map(f -> Files.readString(f)) // ERROR: Unhandled IOException
    .collect(Collectors.toList());
```
You cannot throw a checked exception from a standard `Function<T, R>`. You must wrap it in a `RuntimeException`.

### Try-with-Resources

Java 7 introduced a major DX win for cleanup: **try-with-resources**.

#### The `AutoCloseable` Interface

The mechanism relies on the `AutoCloseable` interface (or its subtype `Closeable`):

```java
public interface AutoCloseable {
    void close() throws Exception;
}
```

Any class implementing this interface can be used in a try-with-resources statement. Common examples: `InputStream`, `OutputStream`, `Reader`, `Writer`, `Connection`, `Statement`, `ResultSet`.

#### Basic Example

```java
// Modern: try-with-resources
public String readFirstLine(String path) throws IOException {
    try (BufferedReader br = new BufferedReader(new FileReader(path))) {
        return br.readLine();
    } // br.close() called automatically here
}
```

**How it works**: The Java compiler automatically inserts a `finally` block that calls `close()` on the resource, even if an exception occurs during the try block.

#### What the Compiler Does (Desugaring)

The above code is **syntactic sugar**. The compiler transforms it into something equivalent to:

```java
public String readFirstLine(String path) throws IOException {
    BufferedReader br = new BufferedReader(new FileReader(path));
    try {
        return br.readLine();
    } finally {
        if (br != null) {
            br.close(); // Called whether readLine() succeeds or throws
        }
    }
}
```

**Key Insight**: It's not a destructor (Java doesn't have those). It's a compile-time transformation that guarantees `close()` is called in a `finally` block.

#### Exception Handling: Suppressed Exceptions

Try-with-resources has sophisticated exception handling. If **both** the try block and `close()` throw exceptions, the exception from the try block is thrown, and the close exception is **suppressed**:

```java
try (Resource r = new Resource()) {
    r.doWork(); // Throws IOException
} // r.close() also throws IOException

// The IOException from doWork() is the primary exception.
// The IOException from close() is added as a "suppressed exception".
```

You can retrieve suppressed exceptions:

```java
try {
    processFile(path);
} catch (IOException e) {
    System.err.println("Primary: " + e.getMessage());
    for (Throwable suppressed : e.getSuppressed()) {
        System.err.println("Suppressed: " + suppressed.getMessage());
    }
}
```

This is a major improvement over manual `finally` blocks, where the close exception would **overwrite** the original exception, losing critical debugging information.

#### Multiple Resources

You can declare multiple resources (separated by semicolons). They are closed in **reverse order** of declaration:

```java
try (FileInputStream fis = new FileInputStream(inputPath);
     FileOutputStream fos = new FileOutputStream(outputPath);
     BufferedReader br = new BufferedReader(new InputStreamReader(fis));
     BufferedWriter bw = new BufferedWriter(new OutputStreamWriter(fos))) {
    
    String line;
    while ((line = br.readLine()) != null) {
        bw.write(line);
        bw.newLine();
    }
} // Closes in order: bw, br, fos, fis
```

#### Comparison: Manual vs. Try-with-Resources

**Before Java 7 (Manual cleanup with nested try-finally)**:

```java
public void copyFile(String src, String dst) throws IOException {
    InputStream in = null;
    OutputStream out = null;
    try {
        in = new FileInputStream(src);
        out = new FileOutputStream(dst);
        
        byte[] buffer = new byte[1024];
        int length;
        while ((length = in.read(buffer)) > 0) {
            out.write(buffer, 0, length);
        }
    } finally {
        if (in != null) {
            try {
                in.close();
            } catch (IOException e) {
                // What do we do here? Log? Ignore? 
                // If 'out' also fails to close, we lose this exception.
            }
        }
        if (out != null) {
            try {
                out.close();
            } catch (IOException e) {
                // Same problem
            }
        }
    }
}
```

**After Java 7 (try-with-resources)**:

```java
public void copyFile(String src, String dst) throws IOException {
    try (InputStream in = new FileInputStream(src);
         OutputStream out = new FileOutputStream(dst)) {
        
        byte[] buffer = new byte[1024];
        int length;
        while ((length = in.read(buffer)) > 0) {
            out.write(buffer, 0, length);
        }
    } // Both streams closed automatically, exceptions properly handled
}
```

**DX Improvement**: Far less boilerplate, no nested try blocks, proper exception chaining automatically.

## Implementation Tradeoffs

### 1. Checked vs. Unchecked
**Tradeoff**: API Stability vs. Evolution.

- **Checked**: Changing implementation details (e.g., adding a DB call) changes the method signature (`throws SQLException`). This breaks all callers.
- **Unchecked**: `RuntimeException`. No signature change, but callers might be surprised by new crashes.
- **Trend**: Modern Java frameworks (Spring, Hibernate) have largely moved to **Unchecked Exceptions** to avoid the "signature pollution" problem.

### 2. Performance: Stack Walking
**Tradeoff**: Exceptions are expensive.

- **Cost**: Creating an exception captures the entire stack trace. This involves walking the stack frames, which is slow.
- **Optimization**: Some high-performance libraries use pre-allocated exceptions without stack traces for control flow, but this is non-standard.

## Summary
Java's Checked Exceptions were a bold experiment in **compile-time safety**. While theoretically sound, the **DX friction** (especially with generics and lambdas) has led most newer languages (Kotlin, C#, Swift) to reject them in favor of Unchecked Exceptions or Result types.
