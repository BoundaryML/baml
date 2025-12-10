# F# Match Syntax

F# (like OCaml) treats pattern matching as a first-class citizen.

## Basic Syntax

```fsharp
let x = 1
match x with
| 1 -> printfn "one"
| 2 -> printfn "two"
| _ -> printfn "other"
```

## Key Features

### 1. Discriminated Unions

F#'s primary data structure for matching.

```fsharp
type Shape =
    | Rectangle of width: float * length: float
    | Circle of radius: float
    | Prism of width: float * float * height: float

let getArea shape =
    match shape with
    | Rectangle(w, l) -> w * l
    | Circle(r) -> System.Math.PI * r * r
    | Prism(w, l, h) -> 2.0 * (w*l + w*h + l*h)
```

### 2. Guards (`when`)

```fsharp
let describe x =
    match x with
    | _ when x < 0 -> "negative"
    | _ when x > 0 -> "positive"
    | _ -> "zero"
```

### 3. Active Patterns

A unique feature of F# that allows you to define custom pattern matching logic.

```fsharp
let (|Even|Odd|) input =
    if input % 2 = 0 then Even else Odd

let testNumber input =
    match input with
    | Even -> printfn "%d is even" input
    | Odd -> printfn "%d is odd" input
```

### 4. List Matching

```fsharp
let rec sumList list =
    match list with
    | [] -> 0
    | head :: tail -> head + sumList tail
```

### 5. Function Keyword

`function` is shorthand for `match` on the last argument.

```fsharp
let rec sumList = function
    | [] -> 0
    | head :: tail -> head + sumList tail
```
