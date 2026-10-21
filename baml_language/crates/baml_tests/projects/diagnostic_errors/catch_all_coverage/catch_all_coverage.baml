// `catch_all` and `catch_all_panics` must cover everything that reaches them
// (E0062); `catch_all_panics` must also cover every panic. B-1787.

class Greeting {
    text: string,
}

class NotFound {
    key: string,
}

function greet(n: int) -> string {
    match (n) {
        0 => throw 1,
        1 => throw "bad",
        2 => throw Greeting { text: "hi" },
        _ => "ok",
    }
}

// B-1787: the `string` arm is missing.
function MissingStringArm() -> string {
    greet(1) catch_all (e) {
        int => "int",
        Greeting => "greeting",
    }
}

// Same, under a `throws never` contract: the catch_all reports the missing
// arm itself, and the contract is not also violated.
function MissingStringArmThrowsNever() -> string throws never {
    greet(1) catch_all (e) {
        int => "int",
        Greeting => "greeting",
    }
}

// A caller of a `catch_all` sees no residual throws.
function CallerOfCatchAll() -> string throws never {
    MissingStringArm()
}

// A `catch_all` after a partial `catch` must cover the residual.
function ChainResidualMissing() -> string {
    greet(1) catch (e) {
        int => "int",
    } catch_all (e2) {
        Greeting => "greeting",
    }
}

// Declared contracts are covered by their widened facts.
function declared(n: int) -> int throws NotFound | string {
    if (n == 0) {
        throw NotFound { key: "k" };
    }
    throw "bad"
}

function MissingDeclared() -> int {
    declared(0) catch_all (e) {
        NotFound => 1,
    }
}

// `catch_all_panics` must cover errors...
function PanicsMissingError() -> string {
    greet(1) catch_all_panics (e) {
        int => "int",
        string => "string",
        baml.panics.Panic => "panic",
    }
}

// ...and every panic: typed error arms alone leave the panics unhandled.
function PanicsMissingPanics() -> string {
    greet(1) catch_all_panics (e) {
        int => "int",
        string => "string",
        Greeting => "greeting",
    }
}

// Exhaustive forms compile.
function Exhaustive() -> string throws never {
    let a = greet(1) catch_all (e) {
        int => "int",
        string => "string",
        Greeting => "greeting",
    };
    let b = greet(1) catch_all (e) {
        _ => "any",
    };
    let c = greet(1) catch_all_panics (e) {
        _ => "any",
    };
    let d = greet(1) catch_all_panics (e) {
        int => "int",
        let rest => "rest",
    };
    a + b + c + d
}

// A refutable arm handles only some values of its type, so it covers none:
// `Greeting { text: "hi" }` leaves other greetings unhandled...
function RefutableErrorArm() -> string {
    greet(1) catch_all (e) {
        int => "int",
        string => "string",
        Greeting { text: "hi" } => "hi",
    }
}

// ...and `IndexOutOfBounds { index: 0 }` leaves other out-of-bounds panics.
function RefutablePanicArm() -> string {
    greet(1) catch_all_panics (e) {
        int => "int",
        string => "string",
        Greeting => "greeting",
        baml.panics.IndexOutOfBounds { index: 0 } => "index 0",
    }
}

// Later exhaustive handlers must cover new errors raised by preceding arms.
function ChainMissingArmError() -> string throws never {
    greet(1) catch (first) {
        _ => throw 42,
    } catch_all (second) {
        string => "not the replacement type",
    }
}

// A partial later catch leaves the preceding arm's error in the contract.
function ChainPartialLeavesArmError() -> string throws never {
    greet(1) catch (first) {
        _ => throw 42,
    } catch (second) {
        string => "not the replacement type",
    }
}
