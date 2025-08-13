function Foo() -> int {
    let x = {
        let y = 1
        y
    }
    x
}

// error: [1;91merror[0m: [1mError validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.[0m
//   [1;94m-->[0m  [4mtest.baml:2[0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction Foo() -> int {
// [1;94m 2 | [0m    let x = [1;91m{[0m
// [1;94m 3 | [0m        let y = 1
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating Function "Foo": This field declaration is invalid. It is either missing a name or a type.[0m
//   [1;94m-->[0m  [4mtest.baml:4[0m
// [1;94m   | [0m
// [1;94m 3 | [0m        let y = 1
// [1;94m 4 | [0m        [1;91my[0m
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: This line is invalid. It does not start with any known Baml schema keyword.[0m
//   [1;94m-->[0m  [4mtest.baml:6[0m
// [1;94m   | [0m
// [1;94m 5 | [0m    }
// [1;94m 6 | [0m    [1;91mx[0m
// [1;94m 7 | [0m}
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Unknown field `let` in function[0m
//   [1;94m-->[0m  [4mtest.baml:2[0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction Foo() -> int {
// [1;94m 2 | [0m    [1;91mlet x = [0m{
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Unknown field `let` in function[0m
//   [1;94m-->[0m  [4mtest.baml:3[0m
// [1;94m   | [0m
// [1;94m 2 | [0m    let x = {
// [1;94m 3 | [0m        [1;91mlet y = 1[0m
// [1;94m   | [0m
// [1;91merror[0m: [1mError validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```[0m
//   [1;94m-->[0m  [4mtest.baml:1[0m
// [1;94m   | [0m
// [1;94m   | [0m
// [1;94m 1 | [0mfunction [1;91mFoo[0m() -> int {
// [1;94m   | [0m