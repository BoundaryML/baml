import { expect, test } from 'bun:test';
import { AppRouterContext } from 'next/dist/shared/lib/app-router-context.shared-runtime';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { IssueActions } from '../src/components/issues/issue-actions.tsx';

test('comments use native form submission while Linear export remains a separate action', () => {
  const html = renderToStaticMarkup(
    React.createElement(
      AppRouterContext.Provider,
      {
        value: { refresh() {} },
      },
      React.createElement(IssueActions, {
        dataset: 'live',
        id: 'ISSUE-1',
        user: 'fixture',
      }),
    ),
  );
  expect(html).toMatch(
    /<form\b[\s\S]*?<button\b[^>]*type="submit"[^>]*>Add comment<\/button>[\s\S]*?<\/form>/,
  );
  expect(html).toMatch(
    /<button\b[^>]*type="button"[^>]*>Export to Linear<\/button>/,
  );
});
