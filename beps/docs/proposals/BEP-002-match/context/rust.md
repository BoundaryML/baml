# Rust Match Syntax

Rust's `match` is a powerful control flow operator that allows you to compare a value against a series of patterns and then execute code based on which pattern matches. Patterns can be made up of literal values, variable names, wildcards, and many other things.

## Basic Syntax

```rust
enum Coin {
    Penny,
    Nickel,
    Dime,
    Quarter,
}

fn value_in_cents(coin: Coin) -> u8 {
    match coin {
        Coin::Penny => 1,
        Coin::Nickel => 5,
        Coin::Dime => 10,
        Coin::Quarter => 25,
    }
}
```

## Key Features

### 1. Exhaustiveness

`match` arms must cover every possibility. If you miss a case, the compiler will error.

```rust
// Error: pattern `None` not covered
match Some(5) {
    Some(x) => println!("{}", x),
}
```

### 2. Destructuring

You can destructure structs, enums, tuples, and references.

```rust
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(i32, i32, i32),
}

match msg {
    Message::Quit => println!("Quit"),
    Message::Move { x, y } => println!("Move to {}, {}", x, y),
    Message::Write(text) => println!("Text: {}", text),
    Message::ChangeColor(r, g, b) => println!("Color: {}, {}, {}", r, g, b),
}
```

### 3. Match Guards

You can add an `if` condition to a match arm.

```rust
match num {
    Some(x) if x < 5 => println!("less than five: {}", x),
    Some(x) => println!("{}", x),
    None => (),
}
```

### 4. Binding with `@`

The `@` operator lets you create a variable that holds a value at the same time as you're testing that value for a pattern match.

```rust
match msg {
    Message::Hello { id: id_variable @ 3..=7 } => {
        println!("Found an id in range: {}", id_variable)
    },
    Message::Hello { id: 10..=12 } => {
        println!("Found an id in another range")
    },
    Message::Hello { id } => {
        println!("Found some other id: {}", id)
    },
}
```

### 5. Expression-based

`match` is an expression, meaning it returns a value.

```rust
let boolean = true;
let binary = match boolean {
    false => 0,
    true => 1,
};
```
