// --------------------------------
// Pure Expression Functions.
// --------------------------------

function ReturnOne() -> int {
    1
}

function ReturnNumber(n: int) -> int {
    n
}

function CallReturnOne() -> int {
    ReturnOne()
}

function ChainedCalls() -> int {
    ReturnNumber(CallReturnOne())
}

function StoreFnCallInLocalVar(n: int) -> int {
    let result = ReturnNumber(n);

    result
}

function BoolToIntWithIfElse(b: bool) -> int {
    if (b) { 1 } else { 0 }
}

function ReturnElseIfExpr(a: bool, b: bool) -> int {
    if (a) {
        1
    } else if (b) {
        2
    } else {
        3
    }
}

function AssignElseIfExpr(a: bool, b: bool) -> int {
    let result = if (a) {
        1
    } else if (b) {
        2
    } else {
        3
    };

    result
}

function NormalElseIfStmt(a: bool, b: bool) -> int {
    let v = 0;

    if (a) {
        let one = 1;
        StoreFnCallInLocalVar(one);
    } else if (b) {
        let two = 2;
        StoreFnCallInLocalVar(two);
    } else {
        let three = 3;
        StoreFnCallInLocalVar(three);
    }

    v
}

function IterativeFibonacci(n: int) -> int {
    let a = 0;
    let b = 1;

    if (n == 0) {
        b
    } else {
        let i = 1;
        while (i <= n) {
            let c = a + b;
            a = b;
            b = c;
            i += 1;
        }
        a
    }
}

function SumArray(arr: int[]) -> int {
    let sum = 0;
    for (let a in arr) {
        sum += a;
    }
    sum
}

function SumFromTo(x: int, y: int) -> int {
    let s = 0;

    for (let i = x; i <= y; i += 1) {
        s += i;
    }

    s
}

function ReturnCategory(category: Category) -> Category {
    category
}

function ReturnImageFromUrl(url: string) -> image {
    image.from_url(url)
}

function HomeEnvVarIsEmpty() -> bool {
    let home = env.HOME;
    home == ""
}

// --------------------------------
// Expression Functions with LLM calls.
// --------------------------------

function CallLlmDescribeImage(img: image) -> string {
    DescribeImage(img)
}

function LlmReturnNumber(n: int) -> int {
    client GPT35LegacyProvider // GPT 3.5 Turbo
    prompt #"
        Return the number {{ n }} without any additional text.
    "#
}

function ReturnNumberCallingLlm(n: int) -> int {
    LlmReturnNumber(n)
}

function StoreLlmCallInLocalVar(n: int) -> int {
    let result = LlmReturnNumber(n);

    result
}

function BoolToIntWithIfElseCallingLlm(b: bool) -> int {
    if (b) { LlmReturnNumber(1) } else { LlmReturnNumber(0) }
}

/// Other builtins

class DummyJsonTodo {
    id int
    todo string
    completed bool
    userId int
}

function ExecFetchAs(url: string) -> DummyJsonTodo {
    let todo = baml.fetch_as<DummyJsonTodo>(url);

    todo
}

function ExecFetchAsWithHttpPostRequest() -> DummyJsonTodo {
    baml.fetch_as<DummyJsonTodo>(baml.HttpRequest {
        method: baml.HttpMethod.Post,
        url: "https://dummyjson.com/todos/add",
        json: {
            "todo": "Buy milk",
            "completed": false,
            "userId": 5
        },
    })
}

// No ID cause API fails
class DummyJsonTodoUpdate {
    todo string
    completed bool
    userId int
}

function ExecFetchAsWithHttpPutRequestAndClassJson() -> DummyJsonTodo {
    let updated_todo = DummyJsonTodoUpdate {
        todo: "Buy milk",
        completed: false,
        userId: 5
    };

    baml.fetch_as<DummyJsonTodo>(baml.HttpRequest {
        method: baml.HttpMethod.Put,
        url: "https://dummyjson.com/todos/1",
        json: updated_todo,
    })
}

test Fib5() {
  functions [IterativeFibonacci]
  args { n 5 }
  @@assert( {{ this == 5 }} )
}
