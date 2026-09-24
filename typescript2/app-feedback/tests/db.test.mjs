import { afterAll, expect, test } from 'bun:test';
import { stageInfo } from '../src/lib/pipeline.ts';

const originalUrl = process.env.FEEDBACK_SUPABASE_URL;
const originalKey = process.env.FEEDBACK_SUPABASE_ANON_KEY;
process.env.FEEDBACK_SUPABASE_URL = 'https://store.invalid';
process.env.FEEDBACK_SUPABASE_ANON_KEY = 'offline-fixture';
const { loadIssuesById, loadPendingReports } = await import('../src/lib/db.ts');
afterAll(() => {
  if (originalUrl === undefined) delete process.env.FEEDBACK_SUPABASE_URL;
  else process.env.FEEDBACK_SUPABASE_URL = originalUrl;
  if (originalKey === undefined) delete process.env.FEEDBACK_SUPABASE_ANON_KEY;
  else process.env.FEEDBACK_SUPABASE_ANON_KEY = originalKey;
});

test('PR agent sessions use the same eval dataset as the activity trail', async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url) => {
    expect(String(url)).toContain('dataset=eq.eval');
    return Response.json([]);
  };
  try {
    const { default: PrPage } = await import(
      '../src/app/prs/[number]/page.tsx'
    );
    const { LiveAgents } = await import('../src/components/live-agents.tsx');
    const page = await PrPage({
      params: Promise.resolve({ number: '123' }),
      searchParams: Promise.resolve({ dataset: 'eval' }),
    });
    const sessions = page.props.children.find(
      (child) => child.type === LiveAgents,
    );
    expect(sessions.props.dataset).toBe('eval');
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('legacy outcome kinds render safely and cancelled issue links are excluded', async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url) => {
    expect(String(url)).toContain('status->>state=neq.cancelled');
    return Response.json([
      {
        comments: [],
        description: '',
        feedback_ids: [],
        id: 'ISSUE-1',
        kind: 'bug',
        outcome: { kind: 'gate_failed' },
        repros: [],
        status: { state: 'open' },
      },
    ]);
  };
  try {
    const [issue] = await loadIssuesById(['ISSUE-1']);
    expect(issue.outcome.kind).toBe('agent_stopped');
    expect(stageInfo(issue).find((s) => s.stage === 'design').state).toBe(
      'failed',
    );
    expect(
      Array.isArray(
        stageInfo({ ...issue, outcome: { ...issue.outcome, kind: 'unknown' } }),
      ),
    ).toBe(true);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('legacy datasets match live history without hiding eval reports with the same ID', async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url) =>
    Response.json(
      String(url).includes('/feedback_public?')
        ? [
            { dataset: null, id: 'FB-1', issue_ids: [], title: 'declined' },
            { dataset: 'eval', id: 'FB-1', issue_ids: [], title: 'eval' },
            { id: 'FB-2', issue_ids: [], title: 'investigating' },
          ]
        : [
            { dataset: null, feedback_id: 'FB-1', kind: 'no_issue' },
            {
              dataset: 'live',
              feedback_id: 'FB-2',
              kind: 'investigation_started',
            },
          ],
    );
  try {
    expect(await loadPendingReports()).toEqual([
      { dataset: 'eval', id: 'FB-1', phase: 'ingested', title: 'eval' },
      {
        dataset: 'live',
        id: 'FB-2',
        phase: 'investigating',
        title: 'investigating',
      },
    ]);
  } finally {
    globalThis.fetch = originalFetch;
  }
});
