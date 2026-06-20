// BAML has no power operator. Writing `2 ** 10` should produce a single,
// clear diagnostic pointing at the unsupported `**` operator and suggesting
// `x.pow(n)`, instead of a confusing two-error cascade.
function p() -> int {
    2 ** 10
}
