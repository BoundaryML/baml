import assert from 'node:assert/strict';
import test from 'node:test';
import { postToSlack } from './clients/slack.js';
import {
  dashboardReportBlocks,
  dashboardReportText,
} from './jobs/send-slack-report.js';

const dashboardUrl = 'https://boundary-product-metrics.fly.dev';
const date = '2026-08-28';
const readmeLink =
  '<https://github.com/BoundaryML/baml/blob/canary/typescript2/app-product-metrics/README.md|docs>';

test('dashboardReportText links the Slack report to the live dashboard', () => {
  assert.ok(
    dashboardReportText(dashboardUrl, date).includes(
      '<https://boundary-product-metrics.fly.dev/|Product metrics dashboard> · 2026-08-28',
    ),
  );
});

test('dashboardReportBlocks renders update guidance in a context block', () => {
  assert.deepEqual(dashboardReportBlocks(dashboardUrl, date), [
    {
      text: {
        text: '<https://boundary-product-metrics.fly.dev/|Product metrics dashboard> · 2026-08-28',
        type: 'mrkdwn',
      },
      type: 'section',
    },
    {
      elements: [
        {
          text: `This report is sent every Friday at 8am PT. To update it, see ${readmeLink}.`,
          type: 'mrkdwn',
        },
      ],
      type: 'context',
    },
  ]);
});

test('postToSlack sends file blocks without an ignored initial comment', async () => {
  const requests: { body: BodyInit | null | undefined; url: string }[] = [];
  const fetchImpl: typeof fetch = async (input, init) => {
    const url = String(input);
    requests.push({ body: init?.body, url });
    if (url.endsWith('/files.getUploadURLExternal')) {
      return Response.json({
        file_id: 'file-id',
        ok: true,
        upload_url: 'https://files.slack.test/upload',
      });
    }
    if (url === 'https://files.slack.test/upload') {
      return new Response(null, { status: 200 });
    }
    return Response.json({ ok: true });
  };
  const blocks = dashboardReportBlocks(dashboardUrl, date);

  await postToSlack(
    'bot-token',
    {
      blocks,
      channel: 'channel-id',
      file: {
        altText: 'Dashboard screenshot',
        bytes: Buffer.from('png'),
        filename: 'dashboard.png',
        title: 'Dashboard',
      },
      text: dashboardReportText(dashboardUrl, date),
    },
    fetchImpl,
  );

  const completion = requests.find((request) =>
    request.url.endsWith('/files.completeUploadExternal'),
  );
  assert.ok(completion);
  const form = new URLSearchParams(String(completion.body));
  assert.equal(form.get('initial_comment'), null);
  assert.deepEqual(JSON.parse(form.get('blocks') ?? 'null'), blocks);
});
