import { expect, test } from 'bun:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { AgentTerminal } from '../src/components/agent-terminal.tsx';
import { Activity } from '../src/components/issues/activity.tsx';
import { githubSourceLink, ticketSections } from '../src/lib/investigation.ts';
import { boardPhase, stageInfo } from '../src/lib/pipeline.ts';

const revision = 'a'.repeat(40);
test('source links point to the recorded revision and reject unsafe paths', () => {
  expect(
    githubSourceLink('baml_language/crates/parser/src/lib.rs:24-31', revision),
  ).toBe(
    `https://github.com/BoundaryML/baml/blob/${revision}/baml_language/crates/parser/src/lib.rs#L24-L31`,
  );
  for (const path of [
    '../secret:1',
    '/etc/passwd:1',
    'https://evil.test:1',
    'src/lib.rs:8-3',
    'src/../x:1',
  ])
    expect(githubSourceLink(path, revision)).toBeNull();
  expect(githubSourceLink('src/lib.rs:1', 'canary')).toBeNull();
});
test('brief description excludes investigation and keeps legacy detail available', () => {
  const sections = ticketSections(
    'First sentence. Second sentence. Third sentence.\n\n## Investigation\n\nThe parser drops a node.',
  );
  expect(sections.brief).toBe('First sentence. Second sentence.');
  expect(sections.investigation).toBe('The parser drops a node.');
  expect(sections.original).toContain('Third sentence.');
});
test('kanban moves with persisted phase and retains terminal status', () => {
  const issue = {
    description: 'No investigation',
    difficulty: null,
    outcome: null,
    shepherd: null,
    status: { state: 'open' },
  };
  expect(boardPhase(issue)).toBe('investigating');
  issue.description = 'Brief.\n\n## Investigation\n\nCause.';
  expect(boardPhase(issue)).toBe('organizing');
  issue.shepherd = 'owner';
  issue.difficulty = 'Easy';
  expect(boardPhase(issue)).toBe('ready');
  issue.outcome = { running: 'design' };
  expect(boardPhase(issue)).toBe('design');
  issue.outcome = { running: 'fix' };
  expect(boardPhase(issue)).toBe('fix');
  issue.outcome = { pr: 'https://github.com/BoundaryML/baml/pull/1' };
  expect(boardPhase(issue)).toBe('pr');
  issue.status = { state: 'shipped' };
  expect(boardPhase(issue)).toBe('shipped');
  issue.status = { pr: null, state: 'in_progress' };
  issue.outcome = { kind: 'agent_stopped' };
  issue.pipeline_phase = 'fix';
  expect(boardPhase(issue)).toBe('fix');
});
test('decision reason is hidden behind a closed summary', () => {
  const html = renderToStaticMarkup(
    React.createElement(Activity, {
      events: [
        {
          created_at: '2026-09-18T00:00:00Z',
          id: 1,
          kind: 'decision',
          payload: {
            decision: 'Bug',
            evidence: {},
            reason: 'Long summary',
            step: 'Investigate',
          },
        },
      ],
    }),
  );
  expect(html.indexOf('<details')).toBeLessThan(html.indexOf('Long summary'));
  expect(html).not.toContain('<details open');
});
test('terminal treats hostile transcript output as text', () => {
  const html = renderToStaticMarkup(
    React.createElement(AgentTerminal, {
      events: [
        {
          continuation: false,
          name: null,
          role: 'assistant',
          text: '<script>alert(1)</script>',
          type: 'text',
        },
      ],
    }),
  );
  expect(html).not.toContain('<script>');
  expect(html).toContain('&lt;script&gt;');
});

test('filenames are linked inside investigation prose, including inline code', async () => {
  const { sourceCitations } = await import('../src/lib/investigation.ts');
  const { FormattedText } = await import('../src/components/code.tsx');
  const citations = sourceCitations(
    ['src/json_parse_state.rs:12-18', 'src/fixing_parser.rs:40-44'],
    revision,
  );
  const html = renderToStaticMarkup(
    React.createElement(FormattedText, {
      citations,
      text: 'The branch in json_parse_state.rs is called by `fixing_parser.rs`.',
    }),
  );
  expect(html).toContain(
    `href="https://github.com/BoundaryML/baml/blob/${revision}/src/json_parse_state.rs#L12-L18"`,
  );
  expect(html).toContain(
    `href="https://github.com/BoundaryML/baml/blob/${revision}/src/fixing_parser.rs#L40-L44"`,
  );
  expect(html).not.toContain('<ul');
  expect(html).toMatch(/<p[^>]*>The branch in <a/);
  const ambiguous = sourceCitations(
    ['src/a/lib.rs:1', 'src/b/lib.rs:2'],
    revision,
  );
  expect(ambiguous.some((c) => c.name === 'lib.rs')).toBe(false);
});

test('intuition entries do not appear in the issue timeline', () => {
  const html = renderToStaticMarkup(
    React.createElement(Activity, {
      events: [
        {
          created_at: '2026-09-18T00:00:00Z',
          id: 1,
          kind: 'intuition',
          payload: { summary: 'Repeated cross-issue intuition' },
        },
      ],
    }),
  );
  expect(html).not.toContain('Repeated cross-issue intuition');
  expect(html).not.toContain('All intuitions');
});

test('MiniATB draft PRs show a completed fix without a legacy handle outcome', () => {
  const issue = {
    description: 'Brief.\n\n## Investigation\nCause.',
    difficulty: 'Easy',
    feedback_ids: [],
    outcome: null,
    repros: [],
    shepherd: 'owner',
    status: {
      pr: 'https://github.com/BoundaryML/baml/pull/42',
      state: 'in_progress',
    },
  };
  const stages = stageInfo(issue);
  expect(stages.find((s) => s.stage === 'fix').state).toBe('done');
  expect(stages.find((s) => s.stage === 'pr').state).toBe('done');
  expect(stages.find((s) => s.stage === 'design').state).toBe('skipped');
});
