import { afterEach, beforeEach, expect, mock, test } from 'bun:test';
import { NextRequest } from 'next/server';

const jar = new Map();
mock.module('server-only', () => ({}));
mock.module('next/headers', () => ({
  cookies: async () => ({
    get: (name) => (jar.has(name) ? { value: jar.get(name) } : undefined),
    set: (name, value) => jar.set(name, value),
  }),
}));
const auth = await import('../src/lib/approval-auth.ts');
const { POST } = await import('../src/app/issues/[id]/approve/route.ts');
const originalFetch = globalThis.fetch;
const names = [
  'FEEDBACK_APPROVAL_SESSION_KEY',
  'FEEDBACK_SITE_URL',
  'FEEDBACK_SUPABASE_URL',
  'FEEDBACK_APPROVAL_SUPABASE_KEY',
];
const originalEnv = Object.fromEntries(names.map((k) => [k, process.env[k]]));
let writes;
let role;
beforeEach(() => {
  jar.clear();
  writes = 0;
  role = 'maintain';
  process.env.FEEDBACK_APPROVAL_SESSION_KEY = 'a'.repeat(64);
  process.env.FEEDBACK_SITE_URL = 'https://feedback.example.invalid';
  process.env.FEEDBACK_SUPABASE_URL = 'https://store.example.invalid';
  process.env.FEEDBACK_APPROVAL_SUPABASE_KEY = 'offline-fixture';
  globalThis.fetch = mock(async (url, init) => {
    if (url === 'https://api.github.com/user')
      return Response.json({ login: 'maintainer' });
    if (url === 'https://api.github.com/repos/BoundaryML/baml')
      return Response.json({ permissions: { [role]: true } });
    if (String(url).startsWith('https://store.example.invalid/')) {
      if (!init.method)
        return Response.json([
          {
            id: 'p1',
            shepherd: role === 'maintain' ? 'maintainer' : 'someone-else',
            status: { state: 'awaiting_approval' },
          },
        ]);
      expect(init.method).toBe('PATCH');
      expect(String(url)).toContain(
        'state=eq.awaiting_approval&shepherd=eq.maintainer',
      );
      expect(String(url)).toContain('dataset=eq.live');
      expect(JSON.parse(init.body).status.by).toBe('github:maintainer');
      writes += 1;
      return Response.json(writes === 1 ? [{ id: 'p1' }] : []);
    }
    throw new Error('Unexpected network call');
  });
});
afterEach(() => {
  globalThis.fetch = originalFetch;
  for (const k of names) {
    if (originalEnv[k] === undefined) delete process.env[k];
    else process.env[k] = originalEnv[k];
  }
});
function request(
  origin = 'https://feedback.example.invalid',
  head = 'b'.repeat(40),
) {
  return new NextRequest('https://feedback.example.invalid/issues/p1/approve', {
    body: new URLSearchParams({ head }),
    headers: { 'Content-Type': 'application/x-www-form-urlencoded', origin },
    method: 'POST',
  });
}
const params = { params: Promise.resolve({ id: 'p1' }) };
test('sessions reject tampering, expiration and the wrong encryption key', () => {
  const sealed = auth.seal({ expires: Date.now() + 60000, token: 'fixture' });
  expect(auth.unseal(sealed)?.token).toBe('fixture');
  const bytes = Buffer.from(sealed, 'base64url');
  bytes[30] ^= 1;
  expect(auth.unseal(bytes.toString('base64url'))).toBeNull();
  expect(auth.unseal(auth.seal({ expires: 1, token: 'fixture' }))).toBeNull();
  process.env.FEEDBACK_APPROVAL_SESSION_KEY = 'c'.repeat(64);
  expect(auth.unseal(sealed)).toBeNull();
});
test('anonymous and cross-origin approvals never write', async () => {
  expect((await POST(request(), params)).status).toBe(403);
  await auth.setSession('fixture');
  expect(
    (await POST(request('https://attacker.example.invalid'), params)).status,
  ).toBe(403);
  expect(writes).toBe(0);
});
test('the assigned shepherd is required', async () => {
  await auth.setSession('fixture');
  role = 'push';
  expect((await POST(request(), params)).status).toBe(403);
  expect(writes).toBe(0);
});
test('approval records verified actor and uses a one-shot conditional write', async () => {
  await auth.setSession('fixture');
  expect((await POST(request(), params)).status).toBe(303);
  expect((await POST(request(), params)).status).toBe(409);
});
test('invalid issue and redirect paths are refused', async () => {
  await auth.setSession('fixture');
  expect(
    (
      await POST(request(), {
        params: Promise.resolve({ id: 'bad&state=eq.approved' }),
      })
    ).status,
  ).toBe(400);
  expect(() => auth.proposalPath('//attacker.invalid')).toThrow();
  expect(writes).toBe(0);
});
test('GitHub authorization failures fail closed', async () => {
  await auth.setSession('fixture');
  globalThis.fetch = mock(
    async () => new Response('unavailable', { status: 503 }),
  );
  expect((await POST(request(), params)).status).toBe(503);
  expect(writes).toBe(0);
});
