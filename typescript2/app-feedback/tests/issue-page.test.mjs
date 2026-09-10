import { expect, mock, test } from 'bun:test';

mock.module('server-only', () => ({}));
mock.module('next/headers', () => ({
  cookies: async () => ({ get: () => undefined, set: () => {} }),
}));
const issue = (id, dataset) => ({
  comments: [],
  created_at: '2026-09-01T00:00:00Z',
  dataset,
  description: '',
  design_doc: null,
  difficulty: null,
  feedback_ids: [],
  id,
  outcome: null,
  repros: [],
  resolution_plan: null,
  shepherd: 'maintainer',
  status: { state: 'awaiting_approval' },
  subsystem: 'Compiler',
  title: 'fixture',
  updated_at: '2026-09-01T00:00:00Z',
  version: '0.17.0',
});
mock.module('../src/lib/db.ts', () => ({
  dataSource: 'mock',
  loadIssue: async (id) =>
    id === 'GH-1' ? issue('GH-1', 'eval') : issue('ISSUE-01k1', 'live'),
  loadIssueEvents: async () => [],
  REVALIDATE_S: 30,
}));
const { ApproveIssue } = await import(
  '../src/components/issues/approve-issue.tsx'
);
const { default: IssuePage } = await import('../src/app/issues/[id]/page.tsx');
async function offersApproval(id) {
  const page = await IssuePage({ params: Promise.resolve({ id }) });
  return [page.props.children]
    .flat()
    .some((child) => child && child.type === ApproveIssue);
}
test('website approval is offered for live issues only, never for eval fixtures', async () => {
  expect(await offersApproval('ISSUE-01k1')).toBe(true);
  expect(await offersApproval('GH-1')).toBe(false);
});
