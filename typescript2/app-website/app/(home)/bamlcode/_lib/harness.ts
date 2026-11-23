import type { Problem, ProblemTest } from './types';

/**
 * Deterministic client-side grading harness.
 *
 * Each test case becomes a zero-arg `baml_language` grader function:
 *
 *   function bc_grade_0() -> void {
 *     assert.equal(TwoSum([2, 7, 11, 15], 9), [0, 1]);
 *   }
 *
 * `assert.equal` compares with `==`, which is structural over arrays, classes
 * and nested values, and throws `baml.panics.AssertionFailed` when the two
 * differ. A passing case returns, and the run finishes with status
 * `succeeded`. A wrong answer, or a solution that throws, ends the run as
 * `failed` or `panicked`. Grading reads ONLY the terminal run status, so no
 * test value is ever decoded.
 *
 * Both parameters of `assert.equal` are `unknown`, so an expected literal the
 * compiler can't type alone (`[]`, `null`) needs no annotation.
 */

export const GRADER_PREFIX = 'bc_grade_';

/** The name of the grader function for the test case at `index`. */
export function graderFnName(index: number): string {
  return `${GRADER_PREFIX}${index}`;
}

export interface GraderCase {
  index: number;
  fnName: string;
  test: ProblemTest;
}

/** One grader case per test in the problem, in order. */
export function graderCases(problem: Problem): GraderCase[] {
  return problem.tests.map((test, index) => ({
    fnName: graderFnName(index),
    index,
    test,
  }));
}

/** The BAML source of one grader function for a test case. */
function graderFn(fnName: string, test: ProblemTest): string {
  return [
    `function ${fnName}() -> void {`,
    `  assert.equal(${test.call}, ${test.expected});`,
    '}',
  ].join('\n');
}

/** The `grader.baml` source: one grader function per test case. */
export function buildGraderFile(problem: Problem): string {
  const header =
    '// AUTO-GENERATED grading harness - not shown to the solver.\n' +
    '// One grader per test case; a wrong answer fails its assertion, which\n' +
    '// fails the run (see harness.ts).\n\n';
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
