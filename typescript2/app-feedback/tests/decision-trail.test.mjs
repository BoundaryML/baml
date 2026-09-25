import { expect, test } from 'bun:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { IntuitionCard } from '../src/components/intuition-card.tsx';
import { Activity } from '../src/components/issues/activity.tsx';

const at = '2026-09-11T19:34:48Z';

test('a decision renders its step, decision, reason and collapsed evidence; unknown kinds are named', () => {
  const html = renderToStaticMarkup(
    React.createElement(Activity, {
      events: [
        {
          created_at: at,
          id: 1,
          kind: 'gauged',
          payload: {
            decision: 'Hard: the cause is unknown',
            evidence: { unknowns: ['which pass drops the entry'] },
            reason: 'Two subsystems <script>x</script>',
            step: 'Gauge difficulty',
          },
          slack_ts: null,
        },
        {
          created_at: at,
          id: 2,
          kind: 'repro_verified',
          payload: { summary: 'legacy' },
          slack_ts: null,
        },
        {
          created_at: at,
          id: 3,
          kind: 'comment_synced',
          payload: {
            author: 'octo-cat',
            summary: "Still broken on today's nightly",
            url: 'https://github.com/BoundaryML/baml/issues/4790#issuecomment-12',
          },
          slack_ts: null,
        },
        {
          created_at: at,
          id: 4,
          kind: 'comment_synced',
          payload: {
            author: 'x',
            summary: 'bad link',
            url: 'https://evil.example/issues/1#issuecomment-1',
          },
          slack_ts: null,
        },
      ],
      issueId: 'ISSUE-1',
    }),
  );
  expect(html).toContain('Difficulty');
  expect(html).toContain('Hard');
  expect(html).toContain('which pass drops the entry');
  expect(html).not.toContain('<script>');
  expect(html).toContain('<details');
  expect(html).not.toContain('Issue status updated');
  expect(html).toContain('Repro verified on the latest nightly');
  expect(html).toContain(
    'https://github.com/BoundaryML/baml/issues/4790#issuecomment-12',
  );
  expect(html).not.toContain('evil.example');
});

test('an intuition card links every cited issue except the current one', () => {
  const html = renderToStaticMarkup(
    React.createElement(IntuitionCard, {
      current: 'ISSUE-a',
      intuition: {
        confidence: 'high',
        evidence: 'ISSUE-a, ISSUE-b',
        generated_at: at,
        id: 'INT-1',
        insight: 'Three tickets in one week.',
        issue_ids: ['ISSUE-a', 'ISSUE-b'],
        kind: 'SharedCause',
        subsystem: 'Runtime',
        suggested_action: 'Audit every bigint arm.',
        title: 'bigint is half-wired into the VM',
      },
    }),
  );
  expect(html).toContain('/issues/ISSUE-b');
  expect(html).not.toContain('/issues/ISSUE-a');
  expect(html).toContain('high confidence');
  expect(html).toContain('Also cites');
});

test('the trail groups steps by phase, folds routine notifications and shows durations', () => {
  const at = (m) => `2026-09-15T00:${String(m).padStart(2, '0')}:00Z`;
  const html = renderToStaticMarkup(
    React.createElement(Activity, {
      events: [
        {
          created_at: at(0),
          id: 1,
          kind: 'ingested',
          payload: { source: 'Github' },
          slack_ts: null,
        },
        {
          created_at: at(1),
          id: 2,
          kind: 'enrich_started',
          payload: { summary: 'Checking' },
          slack_ts: null,
        },
        {
          created_at: at(2),
          id: 3,
          kind: 'decision',
          payload: {
            decision: 'Actionable bug',
            evidence: {},
            reason: 'Concrete repro.',
            step: 'Assess report',
          },
          slack_ts: null,
        },
        {
          created_at: at(14),
          id: 4,
          kind: 'decision',
          payload: {
            decision: 'Subsystem Runtime: x',
            evidence: {},
            reason: 'VM output.',
            step: 'Write the ticket',
          },
          slack_ts: null,
        },
        {
          created_at: at(15),
          id: 5,
          kind: 'gauged',
          payload: {
            decision: 'Medium: a few days',
            evidence: { unknowns: [] },
            reason: 'Known cause.',
            step: 'Gauge difficulty',
          },
          slack_ts: null,
        },
        {
          created_at: at(16),
          id: 6,
          kind: 'slack_notified',
          payload: { summary: 'Slack' },
          slack_ts: null,
        },
      ],
    }),
  );
  for (const label of ['Report', 'Triage', 'Routing'])
    expect(html).toContain(label);
  expect(html).toContain('12 min');
  expect(html).toContain('2 routine notifications folded');
  expect(html).not.toContain('Checking');
  expect(html).toContain('Assess report');
});

test('only the latest triage attempt is shown; earlier aborted attempts are folded', () => {
  const at = (m) => `2026-09-15T00:${String(m).padStart(2, '0')}:00Z`;
  const step = (id, m, step, decision) => ({
    created_at: at(m),
    id,
    kind: 'decision',
    payload: { decision, evidence: {}, reason: 'r', step },
    slack_ts: null,
  });
  const html = renderToStaticMarkup(
    React.createElement(Activity, {
      events: [
        {
          created_at: at(0),
          id: 1,
          kind: 'ingested',
          payload: { source: 'Github' },
          slack_ts: null,
        },
        step(2, 1, 'Assess report', 'first attempt'),
        step(3, 5, 'Write the ticket', 'first ticket'),
        step(4, 10, 'Assess report', 'second attempt'),
        step(5, 14, 'Write the ticket', 'second ticket'),
        step(6, 15, 'Check for duplicates', 'New issue'),
        {
          created_at: at(16),
          id: 7,
          kind: 'gauged',
          payload: {
            decision: 'Easy',
            evidence: {},
            reason: 'r',
            step: 'Gauge difficulty',
          },
          slack_ts: null,
        },
      ],
    }),
  );
  expect(html).toContain('second ticket');
  expect(html).not.toContain('first ticket');
  expect(html).toContain('1 earlier triage attempt hidden');
  expect(html).toContain('Report ingested');
  expect(html).toContain('Difficulty');
});

test('only the latest fix attempt is shown; earlier stopped attempts are counted', () => {
  const at = (m) => `2026-09-15T01:${String(m).padStart(2, '0')}:00Z`;
  const ev = (id, m, kind, payload = {}) => ({
    created_at: at(m),
    id,
    kind,
    payload,
    slack_ts: null,
  });
  const html = renderToStaticMarkup(
    React.createElement(Activity, {
      events: [
        ev(1, 0, 'fix_started'),
        ev(2, 5, 'needs_human', { summary: 'first stop' }),
        ev(3, 10, 'fix_started'),
        ev(4, 15, 'needs_human', { summary: 'second stop' }),
        ev(5, 20, 'fix_started'),
        ev(6, 40, 'pr_opened', {
          pr: 'https://github.com/BoundaryML/baml/pull/4884',
        }),
      ],
    }),
  );
  expect(html).toContain('pull/4884');
  expect(html).not.toContain('first stop');
  expect(html).not.toContain('second stop');
  expect(html).toContain('2 earlier fix attempts hidden');
});
