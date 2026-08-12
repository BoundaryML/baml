// Phase 6 user-facing pin.
//
// Phase 6's stdlib cleanup dropped `type_def: type` from `primitive.parse<T>(body)`
// because the compiler now threads `T`'s runtime type through generic call
// boundaries automatically. The same mechanism lets user code write
// `reflect.type_of<T>()` inside a generic function and have `T` resolve to
// the caller-supplied type at runtime.
//
// This project pins the user-facing TIR/MIR shape of that auto-threading.
// If the substitution machinery regresses, the snapshots here will move.

class Person { name string }

function reflect_type<T>() -> type {
    reflect.type_of<T>()
}

function reflect_array<T>() -> type {
    reflect.type_of<T[]>()
}

function reflect_person() -> type {
    reflect_type<Person>()
}

function reflect_int_array() -> type {
    reflect_array<int>()
}
