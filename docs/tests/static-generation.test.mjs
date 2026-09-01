import assert from 'node:assert/strict';
import test from 'node:test';
import { createDocsPrerenderPredicate } from '../lib/static-generation.mjs';

const shouldPreRenderDocsSlug = createDocsPrerenderPredicate(['0.18.0', '0.17.2']);

test('pre-renders authored and default-version reference pages', () => {
  assert.equal(shouldPreRenderDocsSlug(['baml', 'book', 'prompt-engineering']), true);
  assert.equal(
    shouldPreRenderDocsSlug(['baml', 'language', 'reference', 'baml', 'classes', 'http', 'Request']),
    true,
  );
  assert.equal(shouldPreRenderDocsSlug(['cli', 'commands', 'auth', 'login']), true);
});

test('pre-renders each version landing without multiplying symbol pages', () => {
  assert.equal(shouldPreRenderDocsSlug(['baml', 'language', 'reference', 'v0.18.0']), true);
  assert.equal(shouldPreRenderDocsSlug(['cli', 'commands', 'v0.18.0']), true);

  assert.equal(
    shouldPreRenderDocsSlug([
      'baml',
      'language',
      'reference',
      'v0.18.0',
      'baml',
      'classes',
      'http',
      'Request',
    ]),
    false,
  );
  assert.equal(shouldPreRenderDocsSlug(['cli', 'commands', 'v0.18.0', 'auth', 'login']), false);
});

test('only treats versions from the release catalog as versioned routes', () => {
  assert.equal(
    shouldPreRenderDocsSlug(['baml', 'language', 'reference', 'vercel', 'classes', 'AiGatewayClient']),
    true,
  );
  assert.equal(
    shouldPreRenderDocsSlug(['baml', 'language', 'reference', 'v0.19.0', 'baml', 'classes', 'Array']),
    true,
  );
});
