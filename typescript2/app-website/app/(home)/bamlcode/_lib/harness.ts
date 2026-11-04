import type { Problem, ProblemTest } from './types';

/**
 * Deterministic client-side grading harness.
 *
 * Each test case becomes a zero-arg `baml_language` grader function:
 *
 *   function bc_grade_0() -> int {
 *     let __ok = baml.deep_equals(TwoSum([2, 7, 11, 15], 9), [0, 1]);
 *     [0][if __ok {0} else {1}]
 *   }
 *
 * A passing case indexes `[0][0]` = 0 → the run finishes with status
 * `succeeded`. A wrong answer indexes `[0][1]` → `IndexOutOfBounds`, which
 * propagates to run status `failed`. Grading reads ONLY the terminal run
 * status, so no test value is ever decoded (which the website worker can't do
 * reliably).
 *
 * NB: we do NOT use `assert` - assertion failures (and panics) are soft-caught
 * by the run harness, leaving the status `succeeded` even for wrong answers.
 * `baml.deep_equals` is structural over arrays / classes / nested values
 * (plain `==` on `int[]` is a type error).
 */

export const GRADER_PREFIX = 'bc_grade_';

export function graderFnName(index: number): string {
  return `${GRADER_PREFIX}${index}`;
}

export interface GraderCase {
  index: number;
  fnName: string;
  test: ProblemTest;
}

export function graderCases(problem: Problem): GraderCase[] {
  return problem.tests.map((test, index) => ({
    fnName: graderFnName(index),
    index,
    test,
  }));
}

function graderFn(fnName: string, test: ProblemTest): string {
  // Grade via a status-propagating failure, NOT `assert`: assertion failures
  // (and panics) are soft-caught by the run harness, so the run status stays
  // `succeeded` even for a wrong answer. Instead compare with the structural
  // `baml.deep_equals` (plain `==` on `int[]` is a type error) and index a
  // 1-element array out of bounds on mismatch - `IndexOutOfBounds` DOES
  // propagate to the run status (`failed`), which is what grading reads.
  //
  //   let __ok = baml.deep_equals(<call>, <expected>);
  //   [0][if __ok {0} else {1}]   // ok → [0][0] = 0 ; wrong → [0][1] → throws
  return [
    `function ${fnName}() -> int {`,
    `  let __ok = baml.deep_equals(${test.call}, ${test.expected});`,
    '  [0][if __ok {0} else {1}]',
    '}',
  ].join('\n');
}

/** The `grader.baml` source: one grader function per test case. */
export function buildGraderFile(problem: Problem): string {
  const header =
    '// AUTO-GENERATED grading harness - not shown to the solver.\n' +
    '// One grader per test case; a wrong answer throws IndexOutOfBounds so the\n' +
    '// run status fails (see harness.ts).\n\n';
  return (
    header +
    graderCases(problem)
      .map((c) => graderFn(c.fnName, c.test))
      .join('\n\n') +
    '\n'
  );
}

/** The set of baml_src files for a problem, given the solver's current code. */
export function buildProjectFiles(
  problem: Problem,
  solutionCode: string,
): Record<string, string> {
  const files: Record<string, string> = {
    'grader.baml': buildGraderFile(problem),
    'solution.baml': solutionCode,
  };
  if (problem.prelude && problem.prelude.trim().length > 0) {
    files['prelude.baml'] = problem.prelude;
  }
  return files;
}
