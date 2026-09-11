import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { z } from 'zod';
import { runBamlProcess } from '../lib/snippets/checker';
import {
  discoverProjectSnippets,
  loadProjectSnippet,
} from '../lib/snippets/discovery';
import { parseOperatorArguments } from './operator-arguments';

const args = parseOperatorArguments(process.argv.slice(2), ['baml-bin'], []);
const binary = args.values.get('baml-bin') ?? process.env.BAML_BINARY ?? 'baml';
const projects = await discoverProjectSnippets();
let tests = 0;
for (const project of projects) {
  if (project.expectation.status === 'failure') continue;
  if (!project.files.some((file) => /^test\s+"/m.test(file.displaySource)))
    continue;
  const result = await runBamlProcess(binary, [
    '--color',
    'never',
    '--no-progress',
    '--project',
    project.absolutePath,
    'test',
  ]);
  assert.equal(result.exitCode, 0, `${project.id}\n${result.output}`);
  const summary = /Finished (\d+) passed, 0 failed, (\d+) total/.exec(
    result.output,
  );
  assert.ok(
    summary && Number(summary[1]) > 0,
    `No passing tests executed in ${project.id}:\n${result.output}`,
  );
  assert.equal(summary[1], summary[2], result.output);
  tests += Number(summary[1]);
  console.log(`${project.id}: ${summary[1]} behavioral tests passed`);
}
assert.ok(tests > 0, 'No behavioral tests discovered');

// `shape` is generated from the inferred type, unlike the declaration's body.
// These assertions verify the signatures displayed in Chapter 8 separately
// from executable BAML (a signature without a body is not a runnable example).
const errors = await loadProjectSnippet('listing-08-01');
const signatures = new Map([
  [
    'invoke_tool',
    'function invoke_tool(tool: string) -> string throws baml.errors.InvalidArgument | BadToolInput | ToolUnavailable',
  ],
  [
    'load_tool_config',
    'function load_tool_config(tool: string) -> string throws BadToolInput',
  ],
  [
    'apply_validator',
    'function apply_validator(validate: (score: int) -> int throws callback, score: int) -> int throws callback',
  ],
  [
    'validate_score',
    'function validate_score(score: int) -> int throws BadToolInput',
  ],
]);
// oxlint-disable-next-line anti-slop/no-shape-in-symbol-names -- The BAML describe JSON API names this field shape.
const descriptionSchema = z.object({ shape: z.string() });
const chapter = await readFile(resolve('content/baml/book/errors.mdx'), 'utf8');
for (const [name, expected] of signatures) {
  assert.ok(
    chapter.includes(expected),
    `Displayed signature for ${name} differs from the checked contract`,
  );
  const result = await runBamlProcess(binary, [
    '--project',
    resolve(errors.absolutePath),
    'describe',
    name,
    '--json',
  ]);
  assert.equal(result.exitCode, 0, result.output);
  assert.equal(
    descriptionSchema.parse(JSON.parse(result.stdout)).shape,
    expected,
  );
}
console.log(
  `Validated ${tests} behavioral tests and ${signatures.size} inferred signatures.`,
);
