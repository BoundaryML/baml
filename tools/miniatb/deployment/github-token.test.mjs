import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, verify } from 'node:crypto';
import { installationToken } from './github-token.mjs';
const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const env = { MINIATB_PUBLISH_REPO: 'example/baml-fork', BAMMY_GITHUB_APP_CLIENT_ID: 'test-client', BAMMY_GITHUB_APP_PRIVATE_KEY: privateKey.export({ type: 'pkcs8', format: 'pem' }) };
for (const mode of ['read', 'publish', 'push']) {
  test(`${mode} token is signed, scoped, and kept off redirect targets`, async () => {
    const calls = [];
    const token = await installationToken(mode, env, async (url, options) => {
      calls.push({ url, options });
      assert.equal(options.redirect, 'error');
      const [header, payload, signature] = options.headers.Authorization.slice(7).split('.');
      assert.ok(verify('RSA-SHA256', Buffer.from(`${header}.${payload}`), publicKey, Buffer.from(signature, 'base64url')));
      const claims = JSON.parse(Buffer.from(payload, 'base64url'));
      assert.equal(claims.iss, 'test-client');
      assert.ok(claims.exp - claims.iat <= 600);
      return { ok: true, json: async () => calls.length === 1 ? { id: 123 } : { token: 'test-installation-token' } };
    });
    assert.equal(token, 'test-installation-token');
    assert.equal(calls[0].url, `https://api.github.com/repos/${mode === 'push' ? 'example/baml-fork' : 'BoundaryML/baml'}/installation`);
    assert.equal(calls[1].url, 'https://api.github.com/app/installations/123/access_tokens');
    assert.deepEqual(JSON.parse(calls[1].options.body), {
      repositories: [mode === 'push' ? 'baml-fork' : 'baml'], permissions: mode === 'read' ? { issues: 'read' } : mode === 'push' ? { contents: 'write' } : { contents: 'read', pull_requests: 'write' },
    });
  });
}
test('authentication failures do not expose response bodies', async () => {
  await assert.rejects(installationToken('publish', env, async () => ({ ok: false, status: 403, json: () => { throw new Error('must not read'); } })), /HTTP 403/);
  await assert.rejects(installationToken('publish', {}), /credentials missing/);
  await assert.rejects(installationToken('unknown', env), /Invalid GitHub token mode/);
});

test('push credentials cannot target upstream or an unconfigured repository', async () => {
  for (const repository of [undefined, '', 'BoundaryML/baml', 'boundaryml/other', 'example/baml?x', '../baml']) {
    await assert.rejects(installationToken('push', { ...env, MINIATB_PUBLISH_REPO: repository }, async () => {
      assert.fail('invalid target must never request a token');
    }), /dedicated fork/);
  }
});
