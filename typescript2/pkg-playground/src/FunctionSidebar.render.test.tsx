import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { FunctionSidebar } from './FunctionSidebar';
import type { SerializedTestDef } from './serialized-test-tree';

describe('FunctionSidebar test rows', () => {
  it.each(['pass', 'fail'])(
    'renders the latest %s status immediately before the run action',
    (outcome) => {
      const testTree: SerializedTestDef[] = [
        {
          type: 'test',
          name: 'resume/basic',
        },
      ];
      const markup = renderToStaticMarkup(
        <FunctionSidebar
          functions={[]}
          showInternalFunctions={false}
          internalFunctionCount={0}
          testTree={testTree}
          selectedFn={null}
          onSelectFn={() => {}}
          onRefreshTests={() => {}}
          onRunTest={() => {}}
          testRunResults={new Map([['resume/basic', { outcome }]])}
        />,
      );

      const statusIndex = markup.indexOf(`>${outcome}</span>`);
      const runIndex = markup.indexOf('>run</button>');

      expect(statusIndex).toBeGreaterThan(-1);
      expect(runIndex).toBeGreaterThan(statusIndex);
      expect(markup).toContain(
        `aria-label="Latest test run status: ${outcome}" title="Latest test run status: ${outcome}">${outcome}</span><button`,
      );
    },
  );
});
