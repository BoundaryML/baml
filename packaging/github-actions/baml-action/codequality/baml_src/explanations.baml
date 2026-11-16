/// The collapsed reference at the end of the comment.
/// Every example's score was checked with `baml codequality`.
const EXPLANATIONS = ````
    <details>
    <summary>What these mean</summary>

    - **Cognitive**: how hard the function is to follow. Each branch or loop adds 1, plus 1 more for every level it is nested inside.
    - **Cyclomatic**: how many decisions it makes. 1, plus 1 for each `if`, `else if`, loop, `&&` and `||`.
    - **NPath**: how many paths a test would need to cover. Branches in a row multiply.
    - **Nesting**: how deep the deepest block goes.
    - **Exits**: how many places it can return: each `return` and `throw`, plus the final expression.

    <details>
    <summary>Examples</summary>

    Cognitive 7:

    ```baml
    function capped_total(scores: int[]) -> int {
        let total = 0;
        for (let s in scores) {        // +1
            if (s > 0) {               // +2, nested once
                if (s > 100) {         // +3, nested twice
                    total += 100;
                } else {               // +1
                    total += s;
                }
            }
        }
        total
    }
    ```

    Cognitive 0, with the branching moved into `cap` and the loop into `reduce`:

    ```baml
    function capped_total(scores: int[]) -> int {
        scores.reduce(
            (total, s) -> {
                total + cap(s)
            },
            0,
        )
    }
    ```

    Cyclomatic 5:

    ```baml
    function shipping(region: string, heavy: bool) -> int {
        if (region == "us") {          // +1
            if (heavy) {               // +1
                15
            } else {
                8
            }
        } else if (region == "eu") {   // +1
            if (heavy) {               // +1
                20
            } else {
                12
            }
        } else {
            30
        }
    }
    ```

    NPath 8 (2 x 2 x 2), though cyclomatic is only 4:

    ```baml
    function flags(a: bool, b: bool, c: bool) -> string {
        let s = "";
        if (a) {        // x2
            s += "a";
        }
        if (b) {        // x2
            s += "b";
        }
        if (c) {        // x2
            s += "c";
        }
        s
    }
    ```

    Nesting 3:

    ```baml
    function total_size(rows: Row[]) -> int {
        let count = 0;
        for (let row in rows) {                // 1
            if (row.active) {                  // 2
                for (let cell in row.cells) {  // 3
                    count += cell.size;
                }
            }
        }
        count
    }
    ```

    </details>

    </details>
````;
