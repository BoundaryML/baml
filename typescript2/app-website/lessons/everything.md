# Basics

## Variables

Variables store values that can be reused throughout your BAML code. Use `let` to declare variables:

```baml
// A global variable
let poemSubject = "Math";

// Another global variable
let defaultPoem = ClaudePoem(poemSubject);
```

Variables can store any value type and can reference function calls or other variables.

---

## Annotating Variable Types

You can specify a variable's type. This improves your code readability and improves the
quality of error messages.

```baml
let name: string = "Bammy";
let r: int | string = someComplicatedFunction();
```

---

## Mutating Variables

```baml
function Go() {
    let signups: string[] = [];
    signups.push("Bert")
}
```

It works inside functions & blocks, too:

```baml
function Main() {
    let signups: string[] = [];
    AddBert(signups);
    signups
}

function AddBert(s: string[]) {
    signups.push("Bert");
}
```

---

## Comments

Comments explain your code and are ignored during execution:

```baml
// Single line comment
```

Use header comments to organize complex code paths into blocks:

```baml
function MakeLanguage() -> string {
  //# Initialize
  let expectations = 65536;
  let features = [];

  //# The implementation cycle
  while (features.length() < 10 && expecations > 0) {
    //# Implement a feature.
    features.push(newFeature());

    //# Check the feature set.
    if (HasBugs(features)) {
      expectations -= 1000;
    }

  }

  //# Ship it
  let announcement = if (expectations > 5) {
    "We have a great new language"
  } else {
    "We have a new language!"
  };
  announcement

}
```

# Data Types

## Basic data types

BAML supports several fundamental data types:

```baml
let msg: string = "Hello world";     // String
let answer: int = 42;                // Integer
let pi: float = 3.14;                // Float (not shown in example but supported)
let exists: bool = true;             // Boolean
```

Strings are enclosed in double quotes, integers are whole numbers, and booleans are `true` or `false`.


## Arrays

Arrays store ordered collections of elements of the same type:

```baml
let numbers: int[] = [1, 2, 3, 4, 5];
let names: string[] = ["Alice", "Bob", "Charlie"];
let empty: float[] = [];

// Access elements by index
let first = numbers[0];        // 1
let second = names[1];         // "Bob"
```

Arrays use square brackets and are zero-indexed.

---

## Maps

Maps store key-value pairs with string keys:

```baml
let scores: map<string, int> = {
  "math": 95,
  "science": 87,
  "english": 92
};

// Access values by key
let mathScore = scores["math"]; // 95
```

Maps use curly braces with quoted string keys.

---

## Optional types

Optional types represent values that may or may not be present:

```baml
let maybeName: string? = "Alice";
let maybeAge: int? = null;

// Check if optional has a value
if (maybeName != null) {
  print("Name is: " + maybeName);
}
```

Optional types are denoted with `?` after the type name and can hold either a value or `null`.

---

## Nested structures

You can combine different compound types to create complex data structures:

```baml
let team: string[][] = [
  ["Alice", "Engineer"],
  ["Bob", "Designer"],
  ["Charlie", "Manager"]
];

let user_scores: map<string, int[]> = {
  "alice": [95, 87, 92],
  "bob": [88, 90, 85],
  "charlie": [92, 94, 89]
};

# Classes

## Class definitions

Classes allow you to define your own structured data types with named fields:

```baml
class Todo {
  id int
  todo string
  completed bool
  userId int
}

class Comparison {
  poem1Score int @description("1-10 rating of the first poem's quality")
  poem2Score int @description("1-10 rating of the second poem's quality")
  reasoning string @description("Reasons for the above scores, explicitly contrasting the poems.")

  function topScore(self) -> int {
    if (self.poem1Score > self.poem2Score) {
      self.poem1Score
    } else {
      self.poem2Score
    }
  }
}
```

Classes can include:
- Field names and types
- Methods (like `topScore` above)
- `@description` annotations on fields for documentation
- `@@description` annotation for the whole class

---

## Class usage

Use classes as parameter and return types in functions:

```baml
class Todo {
  id int
  todo string
  completed bool
  userId int
}

function Completed(
  t: Todo
) -> Todo {
    Todo {
        completed: true,
        ..t
    }
}

function TodoContents(t: Todo) -> String {
  // Access fields of a class like this:
  t.todo
}
```

# Functions

## Defining functions

Define a function with the `function` keyword, a list of typed
parameters, a return type, and a function body.

```baml
function Ten() -> int {
  10
}

function Pair(i: int) -> int[] {
    [i, i]
}
```

## Function calls

Call functions by name with arguments in parentheses:

```baml
function RunPoemFaceoff(
  subject: string,
  length: int
) -> Comparison {
  let poem1 = ClaudePoem(subject, length);
  let poem2 = OpenAIPoem(subject, length);
  ComparePoems(subject, poem1, poem2)
}
```

Functions can call other functions and use variables from their scope.

---

## Built-in functions

BAML provides built-in functions for common operations:

```baml
function GetTodo() -> Todo {
  baml.fetch_as<Todo>(baml.Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}
```

The `baml.fetch_as` function makes HTTP requests to external APIs.

# Flow Control

## For loops

### Iterator for loops

For loops iterate over a collection of values. Currently only arrays are supported:

```baml
let names = ["Pete", "Bob", "Sam"];

for (let name in names) {
    print(name)
}
```

Like `while` loops, `for` loops also support `continue` and `break` control flow modifiers:

```baml
let numbers = [5, 10, 2, 0, 3];

let sum = 0;

for (let num in numbers) {
    if (num > 10) {
        continue;
    }
    if (num < 2) {
        break;
    }
    sum += num;
}

print(sum)
```

### C-like for loops

You can also use the traditional c-like `for` loop, specially handy
when part of the loop must be run on `continue`:

```baml
let names = ["Pete", "Bob", "Sam"];

for (let i = 0; i < names.length(); i += 1) {
    print(name[i])
}
```

---

## While loops

While loops continue executing as long as a condition is true:

```baml
let count = 0;
while (count < 5) {
  print(count);
  count = count + 1;
}
```

`continue` and `break` statements are supported:

```baml
while (true) {
  let input = getUserInput();
  if input == "quit" {
    break;
  }

  if input == "hello" {
    sayHello();
    continue;
  }
}
```


While loops check the condition before each iteration.

### Break & Continue

While loops support `break` and `continue` statements:

```baml
let x = 0;

while (x < 10) {
    x += 1;
    if x == 9 {
        continue;
    }
    x += 1;

    let randomNumber = GetRandom(x);

    if randomNumber % 10 == 0 {
        break;
    }
}
```

## If statements

If statements execute code conditionally:

```baml
if (age >= 18) {
  print("Adult");
}

if (score >= 90) {
  print("A grade");
} else if (score >= 80) {
  print("B grade");
} else if (score >= 70) {
  print("C grade");
} else {
  print("Below C");
}
```

---

## Nested control flow

You can combine different control flow statements:

```baml
for (let i: int = 1; i <= 10; i++) {
  if (i % 2 == 0) {
    print("Even: " + i);
  } else {
    print("Odd: " + i);
  }
}

let x = 0;
while (x < 100) {
  if (x % 10 == 0) {
    print("Milestone: " + x);
  }
  x = x + 1;
}
```


You can use `continue` and `break` inside your loops:

---

# Runtime observability

Mark a variable for watching with the `watch` keyword:

```baml
class IntBox {
  inner int
}

function Foo() -> IntBox {
  watch let x = IntBox { inner: 1 };
  x.inner += 1; // This will be noticed by the runtime.
  Double(x);    // So will this.
  x
}

function Double(i: IntBox) -> null {
  i.inner *= 2;
}
```

In client code, subscribe to these events like this:

### TypeScript
```typescript
import {b, watchers} from "../baml_client";

function RunFoo() {
  const watcher = watchers.Foo();
  watther.on_var("x", (ev) => {
    console.log(ev);
  });
  const response = await b.Foo({watchers: watcher});
}
```

### Python
```python
from ../baml_client import b
from ../baml_client import watchers

async def RunFoo():
  watcher = watchers.Foo()
  watcher.on_var("x", lambda ev: print(ev))
  response = await b.Foo({"watchers": watcher})
```

You can change the channel on which updates are published, and the
conditions for publishing, with `baml.WatchOptions`.

```baml
class IntBox {
  inner int
}

function Foo() -> int {
  watch let x = IntBox { inner: 1 };
  x.$watch.options(baml.WatchOptions{channel: "y", when: IsBig});
  x.inner = 2;
  x.inner = 1000; Only this notification will occur.
}

function IsBig(i: IntBox) -> bool {
  i.inner > 100
}
```

---

# Standard Library


## Array

 - `Array.push()`

## Fetch

 - `std.fetch_as<T>()`
