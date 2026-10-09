// Native backend fixture: the `baml.Array` methods that call back into a
// function value, and `cmp` on primitives.

class Item { name: string, weight: int }

function doubled(xs: int[]) -> int[] {
    xs.map((x: int) -> int { x * 2 })
}

function labels(xs: int[]) -> string[] {
    xs.map((x: int) -> string { "n" + x.to_string() })
}

function evens(xs: int[]) -> int[] {
    xs.filter((x: int) -> bool { x % 2 == 0 })
}

function halves(xs: int[]) -> int[] {
    xs.filter_map((x: int) -> int | null { if (x % 2 == 0) { x / 2 } else { null } })
}

function total(xs: int[]) -> int {
    xs.reduce((acc: int, x: int) -> int { acc + x }, 0)
}

function joined(xs: int[]) -> string {
    xs.reduce((acc: string, x: int) -> string { acc + x.to_string() + "," }, "[")
}

function summed(xs: int[]) -> int {
    let acc = 0;
    xs.for_each((x: int) -> void { acc = acc + x; });
    acc
}

function any_big(xs: int[], limit: int) -> bool {
    xs.some((x: int) -> bool { x > limit })
}

function all_big(xs: int[], limit: int) -> bool {
    xs.every((x: int) -> bool { x > limit })
}

function first_big(xs: int[], limit: int) -> int | null {
    xs.find((x: int) -> bool { x > limit })
}

function first_big_at(xs: int[], limit: int) -> int | null {
    xs.find_index((x: int) -> bool { x > limit })
}

function last_big(xs: int[], limit: int) -> int | null {
    xs.find_last((x: int) -> bool { x > limit })
}

function last_big_at(xs: int[], limit: int) -> int | null {
    xs.find_last_index((x: int) -> bool { x > limit })
}

function pairs(xs: int[]) -> int[] {
    xs.flat_map((x: int) -> int[] { [x, x * 10] })
}

function sorted_desc(xs: int[]) -> int[] {
    xs.sort_by((a: int, b: int) -> baml.ops.Ordering { b.cmp(a) })
}

function sorted_by_abs(xs: int[]) -> int[] {
    xs.sort_by_key((x: int) -> int { if (x < 0) { -x } else { x } })
}

function sorted_by_name(items: Item[]) -> string {
    items.sort_by_key((item: Item) -> string { item.name });
    let out = "";
    for (let item in items) { out = out + item.name + ":" + item.weight.to_string() + " "; }
    out
}

function comparisons(xs: int[]) -> int {
    let calls = 0;
    xs.sort_by((a: int, b: int) -> baml.ops.Ordering { calls = calls + 1; a.cmp(b) });
    calls
}

function comparison_log(xs: int[]) -> string {
    let log = "";
    xs.sort_by((a: int, b: int) -> baml.ops.Ordering {
        log = log + a.to_string() + "/" + b.to_string() + " ";
        a.cmp(b)
    });
    log
}

function throwing_comparator(xs: int[]) -> string {
    let seen = 0;
    let sorted = xs.sort_by((a: int, b: int) -> baml.ops.Ordering throws baml.panics.DivisionByZero {
        seen = seen + 1;
        if (seen == 3) { let zero = 0; let boom = a / zero; }
        a.cmp(b)
    }) catch (e) { baml.panics.DivisionByZero => xs };
    sorted.to_string() + " after " + seen.to_string()
}

function mutating_comparator(xs: int[]) -> string {
    xs.sort_by((a: int, b: int) -> baml.ops.Ordering { xs.push(99); a.cmp(b) });
    xs.to_string()
}

function reading_comparator(xs: int[]) -> string {
    let seen = "";
    xs.sort_by((a: int, b: int) -> baml.ops.Ordering { seen = seen + xs[0].to_string(); a.cmp(b) });
    xs.to_string() + " saw " + seen
}

function throwing_key(xs: int[]) -> string {
    let sorted = xs.sort_by_key((x: int) -> int throws baml.panics.DivisionByZero { 100 / x })
        catch (e) { baml.panics.DivisionByZero => xs };
    sorted.to_string()
}

function pushing_map(xs: int[]) -> string {
    let ys = xs.map((x: int) -> int { xs.push(x); x });
    xs.to_string() + " " + ys.to_string()
}

function throwing_map(xs: int[]) -> int {
    xs.map((x: int) -> int throws baml.panics.DivisionByZero { 10 / x }).length()
        catch (e) { baml.panics.DivisionByZero => -1 }
}

function named_callback(xs: int[]) -> int[] {
    xs.map(square)
}

function square(x: int) -> int { x * x }

function with_capture(xs: int[], k: int) -> int[] {
    xs.map((x: int) -> int { x + k })
}

function nullable_mapped(xs: int[]) -> string {
    xs.map((x: int) -> int | null { if (x > 1) { x } else { null } }).to_string()
}

function cmp_ints(a: int, b: int) -> string {
    match (a.cmp(b)) {
        baml.ops.Ordering.Less => "lt",
        baml.ops.Ordering.Equal => "eq",
        baml.ops.Ordering.Greater => "gt",
    }
}

function cmp_floats(a: float, b: float) -> bool {
    a.cmp(b) == baml.ops.Ordering.Less
}

function cmp_strings(a: string, b: string) -> bool {
    a.cmp(b) == baml.ops.Ordering.Greater
}

function render(xs: int[]) -> string {
    doubled(xs).to_string() + " " + evens(xs).to_string() + " " + halves(xs).to_string()
        + " " + pairs(xs).to_string() + " " + sorted_desc(xs).to_string()
        + " " + sorted_by_abs(xs).to_string() + " " + labels(xs).to_string()
        + " " + named_callback(xs).to_string() + " " + with_capture(xs, 7).to_string()
}

function render_items(n: int) -> string {
    sorted_by_name([Item { name: "pear", weight: n }, Item { name: "apple", weight: 1 }, Item { name: "fig", weight: 2 }, Item { name: "apple", weight: 3 }])
}

function of(n: int) -> int[] {
    if (n == 0) { [] } else if (n == 1) { [5] } else if (n == 2) { [3, -1, 4, 1, -5, 9, 2, 6] } else { [7, 7, 1, 7, 2] }
}

function suite(n: int) -> string {
    let xs = of(n);
    render(xs) + " | " + total(xs).to_string() + " " + joined(xs) + " " + summed(xs).to_string()
        + " " + any_big(xs, 3).to_string() + " " + all_big(xs, 0).to_string()
        + " " + first_big(xs, 3).to_string() + " " + first_big_at(xs, 3).to_string()
        + " " + last_big(xs, 3).to_string() + " " + last_big_at(xs, 3).to_string()
        + " " + comparisons(of(n)).to_string() + " " + comparison_log(of(n))
        + " | " + throwing_comparator(of(n)) + " | " + mutating_comparator(of(n))
        + " | " + reading_comparator(of(n)) + " | " + throwing_key(of(n))
        + " | " + pushing_map(of(n)) + " " + throwing_map(of(n)).to_string()
        + " " + nullable_mapped(of(n))
}
