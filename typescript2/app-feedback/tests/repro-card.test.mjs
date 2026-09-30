import { expect, test } from 'bun:test';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { ReproCard } from '../src/components/issues/repro-card.tsx';

const repro = {
  command: 'baml test',
  expectation: { check: 'should_not_compile', diagnostic_contains: null },
  files: { 'regression.baml': 'test "reject" { assert.equal(false, true); }' },
  setup: null,
};

test('one source block precedes collapsed execution output and test harness', () => {
  const html = renderToStaticMarkup(
    React.createElement(ReproCard, {
      repro: {
        ...repro,
        observed: 'assertion failed',
        result: 'fails',
        source_files: { 'example.baml': 'invalid source' },
        source_observed: 'stderr: E0007 <script>alert(1)</script>',
      },
    }),
  );
  expect(html.indexOf('example.baml')).toBeLessThan(html.indexOf('<details'));
  expect(html.indexOf('E0007')).toBeGreaterThan(html.indexOf('<details'));
  expect(html.indexOf('regression.baml')).toBeGreaterThan(
    html.indexOf('<details'),
  );
  expect(html).not.toContain('<details open');
  expect(html).not.toContain('<script>');
  expect(html).toContain('assertion failed');
});

test('legacy repros remain visible without claiming they passed or failed', () => {
  const html = renderToStaticMarkup(React.createElement(ReproCard, { repro }));
  expect(html).toContain('regression.baml');
  expect(html).toContain('Result not classified');
  expect(html).toContain('No execution output recorded');
  expect(html).not.toContain('<details open');
});

test('a passing comparison is distinguished from an unconfirmed example', () => {
  const html = renderToStaticMarkup(
    React.createElement(ReproCard, {
      comparison: true,
      repro: { ...repro, observed: '1 passed', result: 'passes' },
    }),
  );
  expect(html).toContain('Comparison · Passes');
  expect(html).toContain('1 passed');
});

test('unverified outcomes distinguish authoring errors from unavailable verification', () => {
  for (const [result, label] of [
    ['invalid_repro', 'Invalid reproduction'],
    ['unsupported_verification', 'Verification unavailable'],
    ['verification_error', 'Verification failed to run'],
  ]) {
    const html = renderToStaticMarkup(
      React.createElement(ReproCard, { repro: { ...repro, result } }),
    );
    expect(html).toContain(label);
    expect(html).not.toContain('· Fails');
  }
});

test('mixed BAML and Python files retain highlighting in one source block', () => {
  const html = renderToStaticMarkup(
    React.createElement(ReproCard, {
      repro: {
        ...repro,
        files: {
          'main.baml': 'function Example() -> string { return "<script>"; }',
          'test_repro.py':
            'import unittest\n# check result\nassert result == "<script>"',
        },
      },
    }),
  );
  const source = html.slice(0, html.indexOf('<details'));
  expect(source.match(/<pre/g)).toHaveLength(1);
  expect(source).toMatch(/text-purple[^>]+>function<\/span>/);
  expect(source).toMatch(/text-purple[^>]+>import<\/span>/);
  expect(source).toMatch(/text-slate[^>]+># check result<\/span>/);
  expect(source).toContain('&lt;script&gt;');
  expect(source).not.toContain('<script>');
});
