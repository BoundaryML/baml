import { beforeAll, describe, expect, it } from 'vitest';

import { BamlRuntime } from '../runtime/BamlRuntime';
import { DEBUG_BAML_FILES } from '../debugFixtures';

let runtime: BamlRuntime;

beforeAll(async () => {
  const { runtime: realRuntime } = await BamlRuntime.create(
    DEBUG_BAML_FILES,
    {},
    []
  );
  runtime = realRuntime;
});

describe('WASM file grouping', () => {
  it('assigns workflows to their source files', () => {
    const files = runtime.getBAMLFiles();
    const conditionalFile = files.find((file) =>
      file.path.endsWith('baml_src/workflows/conditional.baml')
    );
    expect(conditionalFile, 'conditional.baml entry').toBeTruthy();
    const functionNames = conditionalFile?.functions.map((fn) => fn.name) ?? [];
    expect(functionNames).toContain('ConditionalWorkflow');
  });
});
