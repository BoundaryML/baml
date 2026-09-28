// pending: literal-context: result expectations do not cross indexing or method receivers (00-examples section 8)
// Contextual construction across expression boundaries TYPE_SYSTEM.md
// does not require inference to cross: indexing, and a method receiver.
// Accepting these needs an explicit inference policy (rvalue context
// through the operation's signature), never covariant arrays.
function through_index() -> (int | string)[] throws never {
    [[1]][0]
//   ^^^ (int | string)[]
}

function through_map_index() -> (int | string)[] throws never {
    {key: [1]}["key"]
//        ^^^ (int | string)[]
}

function through_receiver() -> (int | string)[] throws never {
    [1].concat([2])
//  ^^^ (int | string)[]
}
