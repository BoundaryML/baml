import { expect, test } from 'bun:test';
import {
  activityText,
  decisionOf,
  eventProposalPath,
  humanizeKind,
} from '../src/lib/activity.ts';

test('issue and PR lifecycle render without exposing arbitrary payloads', () => {
  for (const kind of [
    'cancelled',
    'issue_created',
    'approved',
    'fix_started',
    'pr_opened',
    'babysit_started',
    'babysit_proposed',
    'babysit_approved',
    'babysit_pushing',
    'babysit_round',
    'babysit_result',
    'unknown',
  ]) {
    const text = activityText({
      kind,
      payload: {
        plan: 'private-fixture',
        result: 'private-fixture',
        summary: 'private-fixture',
      },
    });
    expect(text).not.toContain('private-fixture');
    expect(text.length).toBeGreaterThan(10);
  }
});
test('green is monitoring, not automatic merge or claimed review approval', () => {
  expect(
    activityText({ kind: 'babysit_result', payload: { result: 'green' } }),
  ).toContain('Still watching');
  expect(activityText({ kind: 'approved', payload: {} })).toContain(
    'Shepherd approved',
  );
});
test('proposal links reject control characters and external paths', () => {
  for (const id of [
    '../secret',
    'https://example.invalid',
    'x?next=bad',
    'x\n',
    '',
  ]) {
    expect(eventProposalPath({ payload: { proposal_id: id } })).toBeNull();
  }
  expect(eventProposalPath({ payload: { proposal_id: 'p-123' } })).toBe(
    '/proposals/p-123',
  );
});

test('cancellation is visible in activity', () => {
  expect(activityText({ kind: 'cancelled', payload: {} })).toContain(
    'cancelled',
  );
});

test('pipeline timeline distinguishes stages and confirmed Slack delivery', () => {
  for (const [kind, expected] of [
    ['ingested', 'ingested'],
    ['enrich_started', 'Enrichment'],
    ['enriched', 'Enrichment completed'],
    ['organized', 'organized'],
    ['slack_notified', 'Notified you on Slack'],
    ['babysit_fix_started', 'automatically'],
  ]) {
    expect(activityText({ kind, payload: { source: 'Github' } })).toContain(
      expected,
    );
  }
  expect(activityText({ kind: 'issue_created', payload: {} })).not.toContain(
    'approval',
  );
});

test('a decision renders as its step and parses', () => {
  const gauged = {
    kind: 'gauged',
    payload: {
      decision: 'Hard: the cause is unknown',
      evidence: { unknowns: ['x'] },
      reason: 'Two subsystems.',
      step: 'Gauge difficulty',
    },
  };
  expect(
    activityText({ kind: 'decision', payload: { step: 'Assign a shepherd' } }),
  ).toBe('Assign a shepherd');
  expect(decisionOf(gauged)?.step).toBe('Gauge difficulty');
  expect(decisionOf({ payload: { decision: '', step: 'x' } })).toBeNull();
  expect(humanizeKind('babysit_round')).toBe('Babysit round');
  expect(activityText({ kind: 'some_new_kind', payload: {} })).toBe(
    'Some new kind recorded.',
  );
});

test('synced GitHub comments name a valid login only', () => {
  expect(
    activityText({ kind: 'comment_synced', payload: { author: 'octo-cat' } }),
  ).toContain('@octo-cat');
  expect(
    activityText({ kind: 'comment_synced', payload: { author: '<script>' } }),
  ).not.toContain('<');
});
