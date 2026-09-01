import assert from 'node:assert/strict';
import test from 'node:test';
import nextConfig from '../next.config.mjs';

async function cacheHeaders() {
  assert.equal(typeof nextConfig.headers, 'function');
  const rules = await nextConfig.headers();
  return Object.fromEntries(
    rules.map((rule) => [
      rule.source,
      Object.fromEntries(rule.headers.map(({ key, value }) => [key.toLowerCase(), value])),
    ]),
  );
}

test('serves content-addressed runtime artifacts with immutable caching', async () => {
  const headers = await cacheHeaders();
  assert.equal(
    headers['/baml-runtime/artifacts/:path*']['cache-control'],
    'public, max-age=31536000, immutable',
  );
});

test('revalidates mutable runtime entrypoints', async () => {
  const headers = await cacheHeaders();
  assert.equal(
    headers['/baml-runtime/manifest.json']['cache-control'],
    'public, max-age=0, must-revalidate',
  );
  assert.equal(
    headers['/baml-runtime/runner-worker.mjs']['cache-control'],
    'public, max-age=0, must-revalidate',
  );
});
