// ═══════════════════════════════════════════════════════════════
// COMPREHENSIVE TYPE INFERENCE TEST
// ═══════════════════════════════════════════════════════════════

// ── Enums ───────────────────────────────────────────────────────
enum Color {
    Red,
    Green,
    Blue,
}

enum Status {
    Active,
    Inactive,
    Pending,
}

// ── Classes ─────────────────────────────────────────────────────
class Point {
    x int
    y int
}

class Person {
    name string
    age int
    active bool
}

class Container {
    items int[]
    metadata map<string, int>
}

// ── Fresh/Regular Literal Widening ──────────────────────────────

// 1. return literal 1 against literal return type → should match (no error)
function LiteralReturn() -> 1 {
    return 1;
}

// 2. let widens fresh → x should be int, not Literal(1)
function LetWidening() -> int {
    let x = 1;
    return x;
}

// 3. annotated let preserves literal type
function AnnotatedLet() -> 1 {
    let x: 1 = 1;
    return x;
}

// 4. let widening with string
function StringWidening() -> string {
    let s = "hello";
    return s;
}

// 5. return wrong literal against literal return type → error
function WrongLiteral() -> 1 {
    return 2;
}

// 6. return string literal matching
function StringLiteralReturn() -> "ok" {
    return "ok";
}

// 7. return string literal mismatch → error
function StringLiteralMismatch() -> "ok" {
    return "fail";
}

// ── Arithmetic & Binary Ops ─────────────────────────────────────

// int + int → int
function IntArith(a: int, b: int) -> int {
    return a + b;
}

// fresh literal arithmetic: 1 + 5 on a let-bound var → int
function LiteralArith() -> int {
    let x = 1 + 10 * 10 - 5 / 2;
    x = x + 5;
    return x;
}

// float contaminates: int + float → float
function FloatContamination(a: int, b: float) -> float {
    return a + b;
}

// comparison → bool
function Comparison(a: int, b: int) -> bool {
    return a < b;
}

// logical ops → bool
function LogicalOps(a: bool, b: bool) -> bool {
    return a && b;
}

// string concat
function StringConcat(a: string, b: string) -> string {
    return a + b;
}

// compound assignment: x += 5
function CompoundAssign() -> int {
    let x = 10;
    x += 5;
    return x;
}

// ── Negation ────────────────────────────────────────────────────

function NegateInt(x: int) -> int {
    return -x;
}

function NegateFreshLiteral() -> int {
    let x = -42;
    return x;
}

// ── If/Else ─────────────────────────────────────────────────────

// if/else join: both branches same type
function IfElseSame(cond: bool) -> int {
    if cond { 1 } else { 2 }
}

// if/else join: branches produce different literal values → widened to int
function IfElseWidened(cond: bool) -> int {
    let x = if cond { 1 } else { 2 };
    return x;
}

// if without else → optional
function IfNoElse(cond: bool) -> int? {
    if cond { 42 }
}

// ── Blocks & Tail Expressions ───────────────────────────────────

function BlockTailExpr() -> int {
    let x = {
        let a = 1;
        let b = 2;
        a + b
    };
    return x;
}

// nested blocks
function NestedBlocks() -> string {
    let result = {
        let inner = {
            "hello"
        };
        inner
    };
    return result;
}

// ── Enum Usage ──────────────────────────────────────────────────

function UseEnum() -> Color {
    let c = Color.Red;
    return c;
}

// enum variant in a block
function EnumInBlock() -> Color {
    let c = {
        let x = Color.Green;
        x
    };
    return c;
}

// wrong enum variant → error
function BadEnumVariant() -> Color {
    return Color.Purple;
}

// ── Class Field Access ──────────────────────────────────────────

class Pair {
    first int
    second string

    function Swap(self) -> string {
        return self.second;
    }
}

function ReadField(p: Point) -> int {
    return p.x;
}

// bad field access → error
function BadField(p: Point) -> int {
    return p.z;
}

// ── Array Literals ──────────────────────────────────────────────

function MakeArray() -> int[] {
    return [1, 2, 3];
}

function EmptyArray() -> int[] {
    return [];
}

// mixed array → union
function MixedArray() -> string {
    let arr = [1, "two", 3];
    return "done";
}

// ── Map Literals ────────────────────────────────────────────────

function MakeMap() -> map<string, int> {
    return {"a": 1, "b": 2};
}

// ── Index Access ────────────────────────────────────────────────

function IndexArray(arr: int[]) -> int {
    return arr[0];
}

function IndexMap(m: map<string, int>) -> int {
    return m["key"];
}

// ── Function Calls ──────────────────────────────────────────────

function Identity(x: int) -> int {
    return x;
}

function CallOther() -> int {
    return Identity(42);
}

// ── Type Aliases ────────────────────────────────────────────────

type MyInt = int
type StringOrInt = string | int
type IntList = int[]
type Callback = (x: int) -> int
type Nested = (x: int) -> (y: int) -> int

// recursive type alias
type Tree = int | Tree[]

// ── Function Types ──────────────────────────────────────────────

function GetIdentity() -> (x: int) -> int {
    Identity
}

// ── Match Expression ────────────────────────────────────────────

function MatchColor(c: Color) -> string {
    match (c) {
        Color.Red => "red",
        Color.Green => "green",
        Color.Blue => "blue",
    }
}

function MatchInt(x: int) -> string {
    match (x) {
        1 => "one",
        2 => "two",
        _ => "other",
    }
}

// ── While / For / Break / Continue ──────────────────────────────

function WhileLoop() -> int {
    let x = 0;
    while x < 10 {
        x += 1;
    }
    return x;
}

function ForLoop() -> int {
    let sum = 0;
    for i in [1, 2, 3, 4, 5] {
        sum += i;
    }
    return sum;
}

function EarlyBreak() -> int {
    let x = 0;
    while true {
        x += 1;
        if x > 5 {
            break;
        }
    }
    return x;
}

// ── Dead Code Detection ─────────────────────────────────────────

function DeadCode() -> int {
    return 1;
    let x = 2;
}

// ── Type Mismatch Errors ────────────────────────────────────────

// return string where int expected → error
function ReturnWrongType() -> int {
    return "hello";
}

// return int where string expected → error
function ReturnIntForString() -> string {
    return 42;
}

// annotated let mismatch: let x: string = 1 → error
function AnnotatedLetMismatch() -> string {
    let x: string = 1;
    return x;
}

// ── Union Types ─────────────────────────────────────────────────

function ReturnUnion(cond: bool) -> int | string {
    if cond { 1 } else { "hello" }
}

// ── Optional Types ──────────────────────────────────────────────

function ReturnOptional(cond: bool) -> int? {
    if cond {
        return 42;
    }
    return null;
}

// ── Nested Class Access ─────────────────────────────────────────

function UseContainer(c: Container) -> int {
    return c.items[0];
}

// ── Method Calls ────────────────────────────────────────────────

class Counter {
    value int

    function Increment(self) -> int {
        return self.value + 1;
    }

    function Reset(self) -> int {
        return 0;
    }
}

// ── Assert ──────────────────────────────────────────────────────

function WithAssert(x: int) -> int {
    assert x > 0;
    return x;
}

// ── Evolving Containers ────────────────────────────────────────

// empty array → first push establishes type
function EvolvingArray() -> int {
    let x = [];
    x.push(5);
    x.push(10);
    return x[0];
}

// push wrong type after establishing → error
function EvolvingArrayMismatch() -> int {
    let x = [];
    x.push(5);
    x.push("oops");
    return x[0];
}

// index assignment establishes type
function ArrayIndexAssign() -> int {
    let x = [];
    x[0] = 42;
    x[1] = 100;
    return x[0];
}

// empty map → first bracket assign establishes key+value type
function EvolvingMap() -> int {
    let m = {};
    m["a"] = 1;
    m["b"] = 2;
    return m["a"];
}

// map wrong value type after establishing → error
function EvolvingMapMismatch() -> int {
    let m = {};
    m["a"] = 1;
    m["b"] = "oops";
    return m["a"];
}

// annotated array — push matching type ok
function AnnotatedArrayPush() -> int {
    let x: int[] = [];
    x.push(5);
    return x[0];
}

// annotated array — push wrong type → error
function AnnotatedArrayPushWrong() -> int {
    let x: int[] = [];
    x.push("oops");
    return x[0];
}


// annotated array — push wrong type → error
function UnannotatedArrayPushWrong() -> int {
    let x = [];
    if (true) {
      x.push("oops");
    }
    x.push(1);
    return x[0];
}

// ═══════════════════════════════════════════════════════════════
// HARD MODE: FUNCTION CALL CHAINS & COMPOSITION
// ═══════════════════════════════════════════════════════════════


// ── Basic Call Chains ───────────────────────────────────────────

// f calls g calls h — return type flows back through chain
function Add1(x: int) -> int {
    return x + 1;
}

function Add2(x: int) -> int {
    return Add1(Add1(x));
}

function Add4(x: int) -> int {
    return Add2(Add2(x));
}

// chain with type change at each level
function IntToString(x: int) -> string {
    return "number";
}

function StringToLen(s: string) -> int {
    return 5;
}

// int → string → int
function RoundTrip(x: int) -> int {
    return StringToLen(IntToString(x));
}


// ── Return Type Mismatch Through Chains ─────────────────────────

// Add1 returns int, but we declare string → error
function ChainMismatch(x: int) -> string {
    return Add1(x);
}

// deeper: Add2 returns int, passing through Add1 twice
function DeepChainMismatch(x: int) -> string {
    return Add4(x);
}


// ── Functions Returning Classes ─────────────────────────────────

function MakePoint(x: int, y: int) -> Point {
    return Point { x: x, y: y };
}

// call function that returns class, then access field
function PointX(a: int, b: int) -> int {
    return MakePoint(a, b).x;
}

// nested: function returns class, field used in another call
function PointSum(a: int, b: int) -> int {
    let p = MakePoint(a, b);
    return Add1(p.x) + Add1(p.y);
}

// chained field access through multiple function calls
function MakePerson(n: string, a: int) -> Person {
    return Person { name: n, age: a, active: true };
}

function GetAge(n: string, a: int) -> int {
    return MakePerson(n, a).age;
}


// ── Functions Returning Arrays/Maps + Indexing ──────────────────

function MakeIntArray(a: int, b: int, c: int) -> int[] {
    return [a, b, c];
}

function FirstElement(a: int, b: int, c: int) -> int {
    return MakeIntArray(a, b, c)[0];
}

// function returns map, caller indexes into it
function MakeConfig() -> map<string, int> {
    return {"a": 1, "b": 2};
}

function GetTimeout() -> int {
    return MakeConfig()["timeout"];
}


// ── Functions Returning Enums ───────────────────────────────────

function PickColor(x: int) -> Color {
    if x > 0 { Color.Red } else { Color.Blue }
}

function ColorName(x: int) -> string {
    match (PickColor(x)) {
        Color.Red => "red",
        Color.Green => "green",
        Color.Blue => "blue",
    }
}


// ── Functions as Arguments (Higher-Order) ───────────────────────

function Apply(f: (x: int) -> int, x: int) -> int {
    return f(x);
}

function ApplyTwice(f: (x: int) -> int, x: int) -> int {
    return Apply(f, Apply(f, x));
}

// pass a concrete function
function UseApply() -> int {
    return Apply(Add1, 10);
}

function UseApplyTwice() -> int {
    return ApplyTwice(Add1, 10);
}

// wrong argument type to higher-order function → error
function BadApply() -> int {
    return Apply(IntToString, 10);
}


// ── Functions Returning Functions ────────────────────────────────

function MakeAdder(n: int) -> (x: int) -> int {
    Add1
}

function UseMakeAdder() -> int {
    let f = MakeAdder(5);
    return f(10);
}

// call a returned function immediately
function ImmediateCall() -> int {
    return MakeAdder(5)(10);
}


// ── Compose ─────────────────────────────────────────────────────

function Compose(
    f: (x: int) -> int,
    g: (x: int) -> int,
    x: int
) -> int {
    return f(g(x));
}

function UseCompose() -> int {
    return Compose(Add1, Add2, 0);
}


// ── Method Calls + Call Chains ──────────────────────────────────

class Builder {
    count int
    label string

    function GetCount(self) -> int {
        return self.count;
    }

    function GetLabel(self) -> string {
        return self.label;
    }
}

function MakeBuilder(n: int, s: string) -> Builder {
    return Builder { count: n, label: s };
}

// call function → access method on result
function BuilderCount(n: int) -> int {
    return MakeBuilder(n, "test").GetCount();
}

// chain: function result → field → pass to another function
function BuilderToAdd(n: int) -> int {
    let b = MakeBuilder(n, "hello");
    return Add2(b.count);
}


// ── Nested Container Access Through Calls ───────────────────────

function MakeContainer() -> Container {
    return Container {
        items: [10, 20, 30],
        metadata: {"count": 3, "max": 30},
    };
}

// function returns Container → access items[0]
function ContainerFirst() -> int {
    return MakeContainer().items[0];
}

// function returns Container → access metadata["count"]
function ContainerMeta() -> int {
    return MakeContainer().metadata["count"];
}

// deep: function → field → index → pass to function
function ContainerChain() -> int {
    let c = MakeContainer();
    return Add4(c.items[0]) + c.metadata["max"];
}


// ── Recursive Functions ─────────────────────────────────────────

function Factorial(n: int) -> int {
    if n < 2 {
        return 1;
    }
    return n * Factorial(n - 1);
}

function Fibonacci(n: int) -> int {
    if n < 2 {
        return n;
    }
    return Fibonacci(n - 1) + Fibonacci(n - 2);
}


// ── Mutual Recursion ────────────────────────────────────────────

function IsEven(n: int) -> bool {
    if n == 0 { return true; }
    return IsOdd(n - 1);
}

function IsOdd(n: int) -> bool {
    if n == 0 { return false; }
    return IsEven(n - 1);
}


// ── Complex Type Flows ──────────────────────────────────────────

// result of one call determines the argument type of the next
function Pipeline(x: int) -> string {
    let a = Add4(x);
    let b = MakePoint(a, a);
    let c = b.x + b.y;
    let d = IntToString(c);
    return d;
}

// build an array from function call results
function CollectResults() -> int[] {
    let a = Add1(1);
    let b = Add2(2);
    let c = Add4(4);
    return [a, b, c];
}

// index into an array built from function results, pass to another function
function CollectAndUse() -> int {
    let results = CollectResults();
    return Add1(results[0]) + Add1(results[2]);
}


// ── Errors in Deep Chains ───────────────────────────────────────

// passing wrong arg type deep in chain
function BadFactorialCall() -> int {
    return Factorial("five");
}

// wrong return type propagated through chain
function BadPipelineReturn() -> bool {
    return Pipeline(1);
}

// function returns int, used where string[] expected
function BadCollect() -> string[] {
    return CollectResults();
}


// ── Evolving Containers + Function Calls ────────────────────────

// push results from function calls into evolving array
function EvolveFromCalls() -> int {
    let arr = [];
    arr.push(Add1(0));
    arr.push(Add2(0));
    arr.push(Add4(0));
    return arr[0];
}

// push wrong type from a function call → error
function EvolveMismatchFromCall() -> int {
    let arr = [];
    arr.push(Add1(0));
    arr.push(IntToString(0));
    return arr[0];
}


// ── If/Else with Function Calls ─────────────────────────────────

function BranchCalls(cond: bool, x: int) -> int {
    if cond {
        Add1(x)
    } else {
        Add4(x)
    }
}

// branches return different types → widened to union
function BranchDifferentTypes(cond: bool, x: int) -> int | string {
    if cond {
        Add1(x)
    } else {
        IntToString(x)
    }
}


// ── Match with Function Calls ───────────────────────────────────

function MatchAndCall(c: Color) -> int {
    let base = match (c) {
        Color.Red => 10,
        Color.Green => 20,
        Color.Blue => 30,
    };
    return Add4(base);
}

function MatchCallChain(c: Color) -> int {
    match (c) {
        Color.Red => Add1(0),
        Color.Green => Add2(0),
        Color.Blue => Add4(0),
    }
}


// ── Loop with Function Calls ────────────────────────────────────

function LoopWithCalls() -> int {
    let x = 0;
    let i = 0;
    while i < 5 {
        x = Add2(x);
        i += 1;
    }
    return x;
}

// accumulate function results in evolving array
function LoopCollect() -> int {
    let results = [];
    let i = 0;
    while i < 3 {
        results.push(Add1(i));
        i += 1;
    }
    return results[0];
}


// ── Deeply Nested Call Expressions ──────────────────────────────

// f(g(h(x))) + f(g(h(y)))
function DeepNest(x: int, y: int) -> int {
    return Add1(Add2(Add4(x))) + Add1(Add2(Add4(y)));
}

// function call in array literal in function call argument
function CallInArrayInCall() -> int {
    let arr = [Add1(1), Add2(2), Add4(4)];
    return arr[0] + arr[1] + arr[2];
}

// function call as map value
function CallInMap() -> map<string, int> {
    return {"a": Add1(0), "b": Add2(0), "c": Add4(0)};
}

// function result used as condition
function CallAsCondition(x: int) -> string {
    let x = if (Comparison(x, 10)) {
        return "less";
    } + 10;
    return "not less";
}

// function result used in match scrutinee
function CallInMatchScrutinee(x: int) -> string {
    match (PickColor(x)) {
        Color.Red => "warm",
        Color.Green => "cool",
        Color.Blue => "cool",
    }
}
