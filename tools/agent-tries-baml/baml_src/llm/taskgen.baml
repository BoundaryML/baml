// Task generator — invents HARD algorithm-implementation tasks for the worker to
// solve in pure BAML. Ports services/cron/generator.py. Single-shot -> typed LLM
// call returning a list of task prompts.

class TaskList {
    tasks: string[],
}

function GenerateTasks(recent: string, n: int) -> TaskList {
    client: Sonnet
    prompt: #"
        You invent HARD benchmark tasks for an autonomous agent that writes BAML.

        BAML is a typed language: alongside LLM functions it has plain `function`s with expression bodies, classes with methods, a standard library (Array / String / Map, math), recursion, and control flow. These tasks exercise that general-purpose side -- NOT prompt engineering.

        Each task is a self-contained spec to implement a real ALGORITHM as a BAML `function`. A great task:
        - names an exact signature, e.g. `function edit_distance(a: string, b: string) -> int`;
        - specifies the behavior, the algorithm/method to use, and the edge cases precisely;
        - requires genuine logic -- dynamic programming, recursion, graph traversal, parsing, number theory, combinatorics, geometry, simulation, or a data structure (heap / trie / union-find / ...);
        - lists 2-3 concrete BAML test expectations (input -> exact output) the author MUST include;
        - is solvable in pure BAML with no external IO, and encodes any structured input as a string the function parses (e.g. "u,v,w" edge triples), since inputs are scalars/strings.

        Vary the domain widely across tasks and keep the depth of the two examples below. Do NOT reuse the examples, and do NOT produce anything similar to the prompts in `recent`.

        # Style examples (do not reuse these)
        1. Implement `function count_n_queens(n: int) -> int`: the number of distinct N-Queens solutions via backtracking that places one queen per row and prunes on column and both diagonals. Tests: n=1 -> 1; n=4 -> 2; n=8 -> 92.
        2. Implement `function shortest_distance(n: int, edges: string, source: int, target: int) -> int`: Dijkstra over a weighted directed graph whose `edges` is a space-separated list of "u,v,w" triples; return the shortest distance or -1 if unreachable. Tests: a direct edge wins when shortest; a cheaper two-hop beats a pricier direct edge; unreachable -> -1.

        Invent {{ n }} fresh, varied, hard tasks across different algorithmic domains. Each `tasks` entry is one full self-contained task prompt (one rich paragraph).

        # recent (recently-run tasks to avoid repeating)
        {{ recent }}

        {{ ctx.output_format }}
    "#
}
