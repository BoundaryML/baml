# Go Match Syntax

Go does not have a dedicated `match` expression or structural pattern matching like Rust or Scala. Instead, it uses `switch` statements, which can handle values or types.

## 1. Basic Switch

Go's `switch` is flexible. It doesn't need `break` (it breaks by default), and cases don't need to be constants.

```go
i := 2
switch i {
case 1:
    fmt.Println("one")
case 2:
    fmt.Println("two")
default:
    fmt.Println("other")
}
```

## 2. Switch with no condition

This is a cleaner way to write long `if-else-if` chains.

```go
t := time.Now()
switch {
case t.Hour() < 12:
    fmt.Println("Good morning!")
case t.Hour() < 17:
    fmt.Println("Good afternoon.")
default:
    fmt.Println("Good evening.")
}
```

## 3. Type Switch

This is the closest Go gets to pattern matching on types (like unions). It allows you to switch on the dynamic type of an interface value.

```go
func do(i interface{}) {
    switch v := i.(type) {
    case int:
        fmt.Printf("Twice %v is %v\n", v, v*2)
    case string:
        fmt.Printf("%q is %v bytes long\n", v, len(v))
    default:
        fmt.Printf("I don't know about type %T!\n", v)
    }
}
```

## 4. Select (Channel Matching)

Go has a unique `select` statement for waiting on multiple channel operations. This is a form of pattern matching on communication.

```go
select {
case msg1 := <-c1:
    fmt.Println("received", msg1)
case msg2 := <-c2:
    fmt.Println("received", msg2)
case <-time.After(1 * time.Second):
    fmt.Println("timeout")
}
```

## Limitations vs "Real" Pattern Matching

-   **No Destructuring**: You cannot destructure structs or arrays in a case (e.g., `case Point{x, y}:`).
-   **No Exhaustiveness Checking**: The compiler won't warn you if you miss a case (except for some linter tools).
-   **Statement, not Expression**: `switch` does not return a value.
