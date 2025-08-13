function simpleIf() -> string {
    let x = if true { "yes" } else { "no" }
    x
}

// error: [1;91merror[0m: [1mError validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.[0m
//   [1;94m-->[0m  [4mtest.baml:2[0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction simpleIf() -> string {
// [1;94m 2 | [0m    let x = if true [1;91m{ "yes" } else { "no" }[0m
// [1;94m 3 | [0m    x
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating Function "simpleIf": This field declaration is invalid. It is either missing a name or a type.[0m
//   [1;94m-->[0m  [4mtest.baml:3[0m
// [1;94m   | [0m
// [1;94m 2 | [0m    let x = if true { "yes" } else { "no" }
// [1;94m 3 | [0m    [1;91mx[0m
// [1;94m   | [0m
// [1;91merror[0m: [1mInvalid name for `function`: Must start with an uppercase letter.[0m
//   [1;94m-->[0m  [4mtest.baml:1[0m
// [1;94m   | [0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction [1;91msimpleIf[0m() -> string {
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Unknown field `let` in function[0m
//   [1;94m-->[0m  [4mtest.baml:2[0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction simpleIf() -> string {
// [1;94m 2 | [0m    [1;91mlet x = if true [0m{ "yes" } else { "no" }
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```[0m
//   [1;94m-->[0m  [4mtest.baml:1[0m
// [1;94m   | [0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction [1;91msimpleIf[0m() -> string {
// [1;94m   | [0m