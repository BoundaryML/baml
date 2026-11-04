export type Difficulty = 'Easy' | 'Medium' | 'Hard';

/**
 * A single graded test case. `call` is a BAML expression that invokes the
 * user's function; `expected` is a BAML literal. The grader checks
 * `(call) == (expected)` structurally (see harness.ts). Both are raw
 * `baml_language` source spliced into a grader function body.
 */
export interface ProblemTest {
  call: string;
  expected: string;
  /** Optional human label shown in the results list (defaults to the call). */
  label?: string;
  /** When true, this case is hidden from the problem page (still graded). */
  hidden?: boolean;
}

export interface Problem {
  slug: string;
  id: number;
  title: string;
  difficulty: Difficulty;
  category: string;
  /** Markdown problem statement. */
  statement: string;
  /** The required function signature, shown as a contract. */
  signature: string;
  /** Editor starting code (the signature with a stub body). */
  starter: string;
  /** Reference solution - used to self-validate the problem, never shipped raw. */
  solution: string;
  /**
   * Optional shared `baml_language` declarations (classes/enums like ListNode,
   * TreeNode) placed in their own file so both the user's solution and the
   * grader can reference them.
   */
  prelude?: string;
  tests: ProblemTest[];
}
