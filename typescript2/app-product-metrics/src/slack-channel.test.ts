import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveSlackChannelId } from './clients/slack.js';

test('channel IDs bypass lookup, including private channels', async () => {
  const fetchImpl: typeof fetch = async () => {
    throw new Error('Channel IDs must not require conversations.list');
  };
  for (const channel of [' C07UTQN7N1X ', 'G0123456789']) {
    assert.equal(
      await resolveSlackChannelId(
        'token-without-read-scopes',
        channel,
        fetchImpl,
      ),
      channel.trim(),
    );
  }
});

test('public channel names still resolve through paginated lookup', async () => {
  const requests: URLSearchParams[] = [];
  const fetchImpl: typeof fetch = async (url, init) => {
    assert.equal(url, 'https://slack.com/api/conversations.list');
    requests.push(new URLSearchParams(String(init?.body)));
    return Response.json(
      requests.length === 1
        ? {
            channels: [{ id: 'C0123456789', name: 'other-channel' }],
            ok: true,
            response_metadata: { next_cursor: 'page-two' },
          }
        : { channels: [{ id: 'C0987654321', name: 'general' }], ok: true },
    );
  };
  assert.equal(
    await resolveSlackChannelId('token', ' #general ', fetchImpl),
    'C0987654321',
  );
  assert.equal(requests.length, 2);
  assert.equal(requests[1]?.get('cursor'), 'page-two');
});
