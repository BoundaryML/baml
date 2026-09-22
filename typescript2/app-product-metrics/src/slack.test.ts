import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { resolveSlackChannelId } from './clients/slack.js';

function mockSlackApi(
  responses: unknown[],
  requests: URLSearchParams[],
): typeof fetch {
  return (async (_input, init) => {
    requests.push(new URLSearchParams(String(init?.body)));
    const response = responses.shift();
    assert.notEqual(response, undefined);
    return new Response(JSON.stringify(response), {
      headers: { 'Content-Type': 'application/json' },
      status: 200,
    });
  }) as typeof fetch;
}

describe('resolveSlackChannelId', () => {
  it('normalizes a leading hash and follows pagination', async () => {
    const requests: URLSearchParams[] = [];
    const fetchImpl = mockSlackApi(
      [
        {
          channels: [{ id: 'C1', name: 'general' }],
          ok: true,
          response_metadata: { next_cursor: 'next-page' },
        },
        {
          channels: [{ id: 'C2', name: 'sam-sandbox' }],
          ok: true,
          response_metadata: { next_cursor: '' },
        },
      ],
      requests,
    );

    assert.equal(
      await resolveSlackChannelId('token', '#sam-sandbox', fetchImpl),
      'C2',
    );
    assert.equal(requests.length, 2);
    assert.equal(requests[0]?.get('cursor'), null);
    assert.equal(requests[1]?.get('cursor'), 'next-page');
    assert.equal(requests[1]?.get('types'), 'public_channel');
  });

  it('fails when no public channel has the requested name', async () => {
    const fetchImpl = mockSlackApi(
      [
        {
          channels: [{ id: 'C1', name: 'general' }],
          ok: true,
          response_metadata: { next_cursor: '' },
        },
      ],
      [],
    );

    await assert.rejects(
      resolveSlackChannelId('token', 'missing', fetchImpl),
      /Slack channel #missing was not found/,
    );
  });
});
