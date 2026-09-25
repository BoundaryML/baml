import test from 'node:test';
import assert from 'node:assert/strict';
import { enrich, imageUrl, bounded } from './slack-context.mjs';
import { multimodalInput } from './claude-images.mjs';
const file = { id: 'F1', mimetype: 'image/png', url_private: 'https://files.slack.com/files-pri/T/F/image.png' };
const event = { channel: 'C1', ts: '3.0', thread_ts: '1.0', text: '@bammy fix this', files: [file] };
const json = value => new Response(JSON.stringify(value));
test('paginates prior history, deduplicates images and sends real bytes as media', async () => {
  let calls = 0;
  const result = await enrich(event, 'secret', async (url, options) => {
    assert.equal(options.headers.Authorization, 'Bearer secret');
    assert.equal(options.redirect, 'error');
    calls++;
    if (calls === 1) return json({ ok: true, messages: [{ ts: '1.0', text: 'original report', files: [file] }], response_metadata: { next_cursor: 'next' } });
    if (calls === 2) {
      assert.equal(new URL(url).searchParams.get('cursor'), 'next');
      return json({ ok: true, messages: [{ ts: '2.0', text: 'repro details' }, event, { ts: '4.0', text: 'future' }] });
    }
    return new Response(Buffer.from([137, 80, 78, 71]), { headers: { 'content-type': 'image/png' } });
  });
  assert.equal(calls, 3);
  assert.match(result.description, /original report/);
  assert.match(result.description, /repro details/);
  assert.doesNotMatch(result.description, /future/);
  const wire = JSON.parse(multimodalInput('prompt', result.images));
  assert.equal(wire.message.content[1].source.data, 'iVBORw==');
  assert.equal(wire.message.content[1].source.media_type, 'image/png');
  assert.doesNotMatch(JSON.stringify(wire), /secret|files.slack.com/);
});
test('rejects credential destinations and HTTP errors', async () => {
  for (const url of ['http://files.slack.com/a', 'https://files.slack.com.evil/a', 'https://x@files.slack.com/a', 'https://evil/a']) assert.throws(() => imageUrl(url));
  await assert.rejects(bounded(new Response('bad', { status: 403 }), 10));
  await assert.rejects(bounded(new Response('too long'), 2));
});
test('fails explicitly for unavailable history and oversized images', async () => {
  await assert.rejects(enrich(event, 'token', async () => json({ ok: false, error: 'missing_scope' })), /thread lookup/);
  await assert.rejects(enrich({ ...event, thread_ts: null, files: [{ ...file, size: 99999999 }] }, 'token'), /limits/);
});
test('top-level images download without a thread query', async () => {
  let calls = 0;
  const result = await enrich({ ...event, thread_ts: null }, 'token', async url => {
    calls++; assert.equal(url, file.url_private);
    return new Response('bytes', { headers: { 'content-type': 'image/png' } });
  });
  assert.equal(calls, 1);
  assert.equal(result.images.length, 1);
});
