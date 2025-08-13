function AddOne(x: int) -> int {
    let y = x
    y
}

// error: [1;91merror[0m: [1mError validating Function "AddOne": This field declaration is invalid. It is either missing a name or a type.[0m
//   [1;94m-->[0m  [4mtest.baml:3[0m
// [1;94m   | [0m
// [1;94m 2 | [0m    let y = x
// [1;94m 3 | [0m    [1;91my[0m
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Unknown field `let` in function[0m
//   [1;94m-->[0m  [4mtest.baml:2[0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction AddOne(x: int) -> int {
// [1;94m 2 | [0m    [1;91mlet y = x[0m
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```[0m
//   [1;94m-->[0m  [4mtest.baml:1[0m
// [1;94m   | [0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction [1;91mAddOne[0m(x: int) -> int {
// [1;94m   | [0m