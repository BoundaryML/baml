import type { NextRequest } from 'next/server';
import { NextResponse, after } from 'next/server';
import crypto from 'crypto';
import { WebClient } from '@slack/web-api';
import { submitQuery } from '@/app/actions/query';
import type { Message, QueryRequest } from '@baml/sage-interface';

export const runtime = 'nodejs';

function timingSafeEqual(a: string, b: string): boolean {
  const aBuf = Buffer.from(a, 'utf8');
  const bBuf = Buffer.from(b, 'utf8');
  if (aBuf.length !== bBuf.length) return false;
  return crypto.timingSafeEqual(aBuf, bBuf);
}

function verifySlackSignature({
  rawBody,
  timestamp,
  signature,
  secret,
}: {
  rawBody: string;
  timestamp: string | null;
  signature: string | null;
  secret: string;
}): boolean {
  if (!timestamp || !signature) return false;
  // Reject old timestamps (> 5 minutes)
  const fiveMinutes = 60 * 5;
  const tsNum = Number(timestamp);
  if (!Number.isFinite(tsNum)) return false;
  const now = Math.floor(Date.now() / 1000);
  if (Math.abs(now - tsNum) > fiveMinutes) return false;

  const sigBase = `v0:${timestamp}:${rawBody}`;
  const hmac = crypto.createHmac('sha256', secret).update(sigBase).digest('hex');
  const expected = `v0=${hmac}`;
  return timingSafeEqual(expected, signature);
}

function stripMentionsAndFormatting(text: string): string {
  return text
    .replace(/<@[^>]+>/g, '') // remove mentions
    .replace(/<([^|>]+)\|[^>]+>/g, '$1') // <url|text> -> url
    .replace(/\s+/g, ' ')
    .trim();
}

export async function POST(req: NextRequest) {
  const signingSecret = process.env.SLACK_SIGNING_SECRET;
  const botToken = process.env.SLACK_BOUNDARY_BOT_TOKEN;
  if (!signingSecret || !botToken) {
    return NextResponse.json({ error: 'Slack environment not configured' }, { status: 500 });
  }

  const rawBody = await req.text();
  const timestamp = req.headers.get('x-slack-request-timestamp');
  const signature = req.headers.get('x-slack-signature');

  if (!verifySlackSignature({ rawBody, timestamp, signature, secret: signingSecret })) {
    return NextResponse.json({ error: 'Invalid signature' }, { status: 401 });
  }

  let payload: any;
  try {
    payload = JSON.parse(rawBody);
  } catch (e) {
    return NextResponse.json({ error: 'Invalid JSON' }, { status: 400 });
  }

  // URL verification challenge
  if (payload.type === 'url_verification' && payload.challenge) {
    return new NextResponse(payload.challenge, {
      headers: { 'content-type': 'text/plain' },
    });
  }

  // Acknowledge immediately; process asynchronously
  after(async () => {
    try {
      const event = payload.event;
      if (!event) return;

      // Only handle app mentions and direct messages for now
      if (event.type !== 'app_mention' && !(event.type === 'message' && event.channel_type === 'im')) {
        return;
      }

      const slack = new WebClient(botToken);
      const channel: string = event.channel;
      const thread_ts: string = event.thread_ts || event.ts;
      const text: string = typeof event.text === 'string' ? event.text : '';

      const cleaned = stripMentionsAndFormatting(text);
      if (!cleaned) return;

      // Build prev_messages from thread history (last 10 messages)
      let prev_messages: Message[] = [];
      try {
        const auth = await slack.auth.test();
        const botUserId = auth.user_id;
        const replies = await slack.conversations.replies({ channel, ts: thread_ts, inclusive: true, limit: 10 });
        const msgs = (replies.messages || []) as Array<{ user?: string; bot_id?: string; text?: string; subtype?: string; ts: string }>;
        prev_messages = msgs
          .filter((m) => !m.subtype || m.subtype === 'bot_message')
          .map((m) => {
            const mText = stripMentionsAndFormatting(m.text || '');
            const isAssistant = (m.user && m.user === botUserId) || Boolean(m.bot_id);
            if (isAssistant) {
              return {
                role: 'assistant',
                message_id: `slack-${m.ts}`,
                text: mText,
                ranked_docs: [],
              } as Message;
            }
            return {
              role: 'user',
              text: mText,
            } as Message;
          })
          .slice(0, -1); // exclude current event message
      } catch {}

      const sessionId = `${channel}:${thread_ts}`;
      const request: QueryRequest = {
        session_id: sessionId,
        prev_messages,
        message: {
          role: 'user',
          text: cleaned,
          language_preference: 'en',
        },
      };

      // Optional: send typing indicator (ephemeral)
      try {
        await slack.chat.postEphemeral({ channel, user: event.user, text: 'Thinking…' });
      } catch {}

      const result = await submitQuery(request);

      const answer = result.message.text || 'I could not find an answer.';
      const links = (result.message.ranked_docs || [])
        .slice(0, 3)
        .map((d) => `<${d.url}|${d.title}>`)
        .join('  •  ');

      const finalText = links ? `${answer}\n\nSources: ${links}` : answer;

      await slack.chat.postMessage({
        channel,
        text: finalText,
        thread_ts,
      });
    } catch (err) {
      console.error('Slack event handling failed:', err);
    }
  });

  return NextResponse.json({ ok: true });
}