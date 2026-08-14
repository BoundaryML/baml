// E0125: class implements Person which requires Named and Aged,
// but does not implement Named or Aged.

interface Named {
    name: string
}

interface Aged {
    age: int
}

interface Person requires Named, Aged {
    occupation: string
}

class Bad {
    occupation: string

    implements Person {
    }
}

//----
//- diagnostics
// E0125
//
//   × implementing `Person` also requires implementing `Named`
//     ╭─[interface_error_missing_required_interface.baml:19:16]
//  19 │     implements Person {
//     ·                ──────
//     ╰────
// E0125
//
//   × implementing `Person` also requires implementing `Aged`
//     ╭─[interface_error_missing_required_interface.baml:19:16]
//  19 │     implements Person {
//     ·                ──────
//     ╰────
