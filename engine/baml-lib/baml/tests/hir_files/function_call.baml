function myFunc(x: int, y: string) -> int {
    x
}

function CallTest() -> int {
    let result = myFunc(42, "hello")
    result
}

// error: [1;91merror[0m: [1mError validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.[0m
//   [1;94m-->[0m  [4mtest.baml:6[0m
// [1;94m   | [0m
// [1;94m 5 | [0mfunction CallTest() -> int {
// [1;94m 6 | [0m    let result = myFunc[1;91m(42, "hello")[0m
// [1;94m 7 | [0m    result
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating Function "CallTest": This field declaration is invalid. It is either missing a name or a type.[0m
//   [1;94m-->[0m  [4mtest.baml:7[0m
// [1;94m   | [0m
// [1;94m 6 | [0m    let result = myFunc(42, "hello")
// [1;94m 7 | [0m    [1;91mresult[0m
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Unknown field `let` in function[0m
//   [1;94m-->[0m  [4mtest.baml:6[0m
// [1;94m   | [0m
// [1;94m 5 | [0mfunction CallTest() -> int {
// [1;94m 6 | [0m    [1;91mlet result = myFunc[0m(42, "hello")
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```[0m
//   [1;94m-->[0m  [4mtest.baml:5[0m
// [1;94m   | [0m
// [1;94m 4 | [0m
// [1;94m 5 | [0mfunction [1;91mCallTest[0m() -> int {
// [1;94m   | [0m