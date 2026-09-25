import { expect, test } from 'bun:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { FeedbackList, fateOf } from '../src/components/feedback-list.tsx';

const issue = {
  comments: [],
  created_at: '2026-09-17T18:40:00Z',
  dataset: 'live',
  description: '',
  design_doc: null,
  difficulty: null,
  feedback_ids: ['FB-min-4901'],
  id: 'ISSUE-1',
  kind: 'bug',
  outcome: null,
  repros: [],
  resolution_plan: null,
  shepherd: null,
  status: { state: 'open' },
  subsystem: 'Runtime',
  title: 'host callable returning a whole number for a float panics',
  updated_at: '2026-09-17T18:40:00Z',
  version: '0.19.0',
};
const base = {
  body: 'Node bridge 0.19.0. My JS host function returns 612 and the call panics.',
  dataset: 'live',
  received_at: '2026-09-17T18:37:45Z',
  source: 'BamlFeedback',
  toolchain: '0.19.0',
};
const reports = [
  {
    ...base,
    id: 'FB-min-4901',
    issue_ids: ['ISSUE-1'],
    issues: [issue],
    no_issue: null,
    title: 'host callable panics',
  },
  {
    ...base,
    id: 'FB-min-4765',
    issue_ids: [],
    issues: [],
    no_issue: 'fixed on this toolchain',
    title: 'escapes kept raw',
  },
  {
    ...base,
    dataset: 'eval',
    id: 'FB-min-4620',
    issue_ids: [],
    issues: [],
    no_issue: null,
    title: 'openai compatible host',
  },
];

test('each report shows what it became: the issue, the no-issue reason, or that it is waiting', () => {
  const html = renderToStaticMarkup(
    React.createElement(FeedbackList, { reports }),
  );
  expect(html).toContain('href="/feedback/FB-min-4901"');
  expect(html).toContain('href="/issues/ISSUE-1"');
  expect(html).toContain('No issue:');
  expect(html).toContain('fixed on this toolchain');
  expect(html).toContain('Waiting for triage.');
  expect(html).toContain(
    '3 reports · 1 became an issue · 1 no issue · 1 waiting for triage',
  );
  expect(html).toContain('>eval<');
  expect(html).not.toContain('device_id');
});

test('fateOf prefers an issue over a recorded reason', () => {
  expect(fateOf({ issues: [issue], no_issue: 'stale' })).toBe('issue');
  expect(fateOf({ issues: [], no_issue: 'needs_details' })).toBe('no_issue');
  expect(fateOf({ issues: [], no_issue: null })).toBe('pending');
});

test('an empty store renders a sentence, not an empty list', () => {
  expect(
    renderToStaticMarkup(React.createElement(FeedbackList, { reports: [] })),
  ).toContain('No reports yet.');
});
