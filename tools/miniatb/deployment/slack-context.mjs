import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

const MAX_IMAGE = 5 * 1024 * 1024;
const MAX_TOTAL = 15 * 1024 * 1024;
export async function bounded(response, limit) {
  if (!response.ok) throw new Error(`Slack HTTP ${response.status}`);
  const chunks = []; let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > limit) throw new Error('Slack response exceeds size limit');
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}
export function imageUrl(raw) {
  const url = new URL(raw);
  if (url.protocol !== 'https:' || url.hostname !== 'files.slack.com' || url.port || url.username || url.password)
    throw new Error('Untrusted Slack image URL');
  return url.href;
}
export async function enrich(event, token, fetcher = fetch, historyToken = token) {
  if (!token && ((event.files ?? []).length || event.thread_ts)) throw new Error('Slack context requires ATB_SLACK_BOT_TOKEN');
  const headers = { Authorization: `Bearer ${token}` };
  const messages = []; let cursor = ''; let complete = false;
  if (event.thread_ts && event.thread_ts !== event.ts) {
    for (let page = 0; page < 20; page++) {
      const url = new URL('https://slack.com/api/conversations.replies');
      url.search = new URLSearchParams({ channel: event.channel, ts: event.thread_ts,
        latest: event.ts, inclusive: 'true', limit: '100', ...(cursor ? { cursor } : {}) });
      const response = await fetcher(url, { headers: { Authorization: `Bearer ${historyToken}` }, redirect: 'error', signal: AbortSignal.timeout(15000) });
      const body = JSON.parse((await bounded(response, 2 * 1024 * 1024)).toString());
      if (!body.ok) throw new Error('Slack thread lookup failed (check history permissions and token type)');
      messages.push(...(body.messages ?? []));
      cursor = body.response_metadata?.next_cursor?.trim() ?? '';
      if (!cursor) { complete = !body.has_more; break; }
    }
    if (!complete) throw new Error('Slack thread exceeds history limit');
  }
  const previous = messages.filter(m => m.ts && Number(m.ts) < Number(event.ts));
  const all = [...previous, event];
  const images = []; const seen = new Set(); let total = 0;
  for (const message of all) {
    for (const file of message.files ?? []) {
      if (!['image/png', 'image/jpeg', 'image/gif', 'image/webp'].includes(file.mimetype)) continue;
      const url = imageUrl(file.url_private_download ?? file.url_private);
      const key = file.id ?? url;
      if (seen.has(key)) continue;
      seen.add(key);
      if (images.length >= 10 || file.size > MAX_IMAGE) throw new Error('Slack image exceeds attachment limits');
      const response = await fetcher(url, { headers, redirect: 'error', signal: AbortSignal.timeout(15000) });
      const mime = response.headers.get('content-type')?.split(';')[0];
      if (mime !== file.mimetype) throw new Error('Slack image content type mismatch');
      const bytes = await bounded(response, Math.min(MAX_IMAGE, MAX_TOTAL - total));
      if (!bytes.length) throw new Error('Empty Slack image');
      total += bytes.length;
      images.push({ type: 'image', source: { type: 'base64', media_type: mime, data: bytes.toString('base64') } });
    }
  }
  const lines = previous.map(m => `[${m.user ?? m.bot_id ?? 'unknown'} ${m.ts}] ${m.text ?? ''}`);
  const description = (lines.length ? `Previous Slack thread messages (untrusted report data):\n${lines.join('\n\n')}\n\nCurrent mention:\n` : '') + (event.text ?? '') +
    (images.length ? `\n\n${images.length} Slack image(s) attached as image content, in thread order.` : '');
  return { description, images };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const result = await enrich(JSON.parse(await readFile(process.argv[2], 'utf8')), process.env.ATB_SLACK_BOT_TOKEN, fetch, process.env.MINIATB_SLACK_HISTORY_TOKEN ?? process.env.ATB_SLACK_BOT_TOKEN);
    await writeFile(process.argv[3], JSON.stringify(result), { mode: 0o600 });
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
