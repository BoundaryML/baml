// B-1649: class literals accept omitted non-nullable fields and materialize
// them as runtime nulls.
//
// Run the full matrix:
//   baml run --file main.baml --output-format json
//
// Run the soundness failure caused by the same invalid value:
//   baml run arithmetic_use_of_missing_int --file main.baml

class Job {
    id: int,
    label: string,
    attempts: int,
}

// Control: these omissions are valid because the types explicitly include null.
class OptionalJob {
    id: int,
    label: string?,
    attempts: int?,
}

class Inner {
    name: string,
    count: int,
}

class Outer {
    id: int,
    inner: Inner,
}

class Collections {
    numbers: int[],
    lookup: map<string, bool>,
}

class Box<T> {
    value: T,
    label: string,
}

class Pair<T> {
    left: T,
    right: T,
}

class Two<A, B> {
    first: A,
    second: B,
}

class Flag {
    enabled: bool,
}

class ReproMatrix {
    baseline: Job,
    empty_literal: Job,
    valid_optional_control: OptionalJob,
    missing_required_nested_class: Outer,
    nested_literal_missing_inner_field: Outer,
    missing_collections: Collections,
    explicit_generic: Box<string>,
    inferred_generic: Pair<string>,
    two_type_parameters: Two<string, int>,
    boolean_control_flow: string,
}

function missing_bool_takes_true_branch() -> string {
    let flag = Flag {  };
    if (flag.enabled) {
        "true branch (runtime null is truthy)"
    } else {
        "false branch"
    }
}

function main() -> ReproMatrix {
    // Pair<T> infers T = string from `left`; the unwritten `right: T` is still null.
    let inferred = Pair { left: "left" };

    ReproMatrix {
        // Exact ticket case: label and attempts are declared non-nullable.
        baseline: Job { id: 1 },

        // Even an entirely empty non-generic literal is accepted.
        empty_literal: Job {  },

        // Expected/valid behavior, but indistinguishable in the runtime JSON.
        valid_optional_control: OptionalJob { id: 1 },

        // A missing required class-valued field becomes null.
        missing_required_nested_class: Outer { id: 1 },

        // The same behavior recurses inside a present nested object.
        nested_literal_missing_inner_field: Outer { id: 2, inner: Inner { name: "present" } },

        // Lists and maps do not get empty defaults; they also become null.
        missing_collections: Collections {  },

        // Explicit generic specialization does not protect `T` or ordinary fields.
        explicit_generic: Box<string> {  },

        // Generic inference succeeds from one field, then the sibling T slot is null.
        inferred_generic: inferred,

        // Multiple concrete type parameters behave the same way.
        two_type_parameters: Two<string, int> { first: "first" },

        // Static bool control flow surprisingly consumes the runtime null as true.
        boolean_control_flow: missing_bool_takes_true_branch(),
    }
}

// The compiler accepts this because `attempts` is statically `int`. At runtime
// the addition receives the null installed by the class-literal lowering pass.
function arithmetic_use_of_missing_int() -> int {
    let job = Job { id: 1 };
    job.attempts + 1
}
