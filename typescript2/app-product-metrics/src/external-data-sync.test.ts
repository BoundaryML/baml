import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { test } from 'node:test';
import {
  type ExternalDataSyncRequest,
  parseExternalDataSyncRequest,
} from './external-data.js';
import type { Prisma, PrismaClient } from './generated/prisma/client.js';
import { createRequestHandler } from './http.js';
import { fetchLumaEapData } from './jobs/fetch-luma-eap.js';
import { fetchSheepCouncilZoomData } from './jobs/fetch-sheep-council-zoom.js';

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    headers: { 'Content-Type': 'application/json' },
    status,
  });
}

function fakePrisma() {
  const writes: Prisma.PlgWeeklyMetricRawCreateArgs[] = [];
  const prisma = {
    plgWeeklyMetricRaw: {
      async create(args: Prisma.PlgWeeklyMetricRawCreateArgs) {
        writes.push(args);
        return args.data;
      },
    },
  } as unknown as PrismaClient;
  return { prisma, writes };
}

const zoomConfig = {
  accountId: 'zoom-account',
  clientId: 'zoom-client',
  clientSecret: 'zoom-secret',
};

test('fetchSheepCouncilZoomData stores only instances in the requested Monday-to-Monday window', async () => {
  const requests: string[] = [];
  const fetchImpl: typeof fetch = async (input) => {
    const url = String(input);
    requests.push(url);
    if (url.startsWith('https://zoom.us/oauth/token')) {
      return jsonResponse({ access_token: 'access-token' });
    }
    if (url.endsWith('/past_meetings/89180085282/instances')) {
      return jsonResponse({
        meetings: [
          { start_time: '2026-08-24T16:00:00Z', uuid: 'inside+uuid==' },
          { start_time: '2026-08-18T16:00:00Z', uuid: 'outside-uuid' },
        ],
      });
    }
    if (url.includes('/participants?')) {
      return jsonResponse({ next_page_token: '', participants: [] });
    }
    throw new Error(`Unexpected request: ${url}`);
  };
  const { prisma, writes } = fakePrisma();
  const result = await fetchSheepCouncilZoomData(
    {
      meetingId: '89180085282',
      timeZone: 'America/Los_Angeles',
      zoom: zoomConfig,
    },
    prisma,
    { end: '2026-08-31', start: '2026-08-24' },
    new Date('2026-08-27T12:00:00Z'),
    fetchImpl,
  );

  assert.equal(result.instanceCount, 1);
  assert.equal(result.weekStartDate.toISOString(), '2026-08-24T07:00:00.000Z');
  assert.equal(writes.length, 1);
  assert.equal(writes[0]?.data.rawMetricType, 'SHEEP_COUNCIL_MEETINGS');
  assert.equal(
    requests.filter((url) => url.includes('/participants?')).length,
    2,
  );
  assert.ok(!requests.some((url) => url.includes('outside-uuid/participants')));
  const raw = writes[0]?.data.rawMetricData as Record<string, unknown>;
  const zoom = raw.zoom as { captures: unknown[] };
  assert.equal(zoom.captures.length, 1);
});

test('fetchLumaEapData resolves matching Luma events to Zoom instances without storing join passwords', async () => {
  const fetchImpl: typeof fetch = async (input) => {
    const url = String(input);
    if (url.startsWith('https://zoom.us/oauth/token')) {
      return jsonResponse({ access_token: 'access-token' });
    }
    if (url.includes('/v1/calendars/events/list?')) {
      return jsonResponse({
        entries: [
          {
            end_at: '2026-08-27T18:00:00Z',
            id: 'eap-event',
            location_type: 'zoom',
            meeting_url: 'https://us02web.zoom.us/j/123456789?pwd=do-not-store',
            name: 'BAML Early Access',
            start_at: '2026-08-27T17:00:00Z',
          },
          {
            end_at: '2026-08-28T18:00:00Z',
            id: 'different-event',
            location_type: 'zoom',
            meeting_url: 'https://zoom.us/j/999',
            name: 'Something Else',
            start_at: '2026-08-28T17:00:00Z',
          },
        ],
        has_more: false,
      });
    }
    if (url.includes('/v1/events/guests/list?')) {
      return jsonResponse({
        entries: [{ approval_status: 'approved', email: 'person@example.com' }],
        has_more: false,
      });
    }
    if (url.endsWith('/past_meetings/123456789/instances')) {
      return jsonResponse({
        meetings: [
          { start_time: '2026-08-27T17:01:00Z', uuid: 'eap-instance' },
        ],
      });
    }
    if (url.includes('/participants?')) {
      return jsonResponse({ next_page_token: '', participants: [] });
    }
    throw new Error(`Unexpected request: ${url}`);
  };
  const { prisma, writes } = fakePrisma();
  const result = await fetchLumaEapData(
    {
      eventName: 'BAML Early Access',
      lumaApiKey: 'luma-key',
      timeZone: 'America/Los_Angeles',
      zoom: zoomConfig,
    },
    prisma,
    { end: '2026-08-31', start: '2026-08-24' },
    new Date('2026-08-27T12:00:00Z'),
    fetchImpl,
  );

  assert.equal(result.eventCount, 1);
  assert.equal(result.resolvedEventCount, 1);
  assert.equal(writes.length, 1);
  assert.equal(writes[0]?.data.rawMetricType, 'EAP_MEETINGS');
  const serialized = JSON.stringify(writes[0]?.data.rawMetricData);
  assert.ok(serialized.includes('123456789'));
  assert.ok(serialized.includes('person@example.com'));
  assert.ok(!serialized.includes('do-not-store'));
});

test('parseExternalDataSyncRequest requires an exact Monday-to-Monday request', () => {
  assert.deepEqual(
    parseExternalDataSyncRequest({
      raw_metric_period: { end: '2026-08-31', start: '2026-08-24' },
      raw_metric_type: 'sheep-council-meetings',
    }),
    {
      raw_metric_period: { end: '2026-08-31', start: '2026-08-24' },
      raw_metric_type: 'sheep-council-meetings',
    },
  );
  assert.throws(
    () =>
      parseExternalDataSyncRequest({
        raw_metric_period: { end: '2026-09-01', start: '2026-08-25' },
        raw_metric_type: 'eap-meetings',
      }),
    /Mondays/,
  );
});

test('POST /sync-external-data requires the request body without authentication', async () => {
  let invocationCount = 0;
  let receivedRequest: ExternalDataSyncRequest | undefined;
  const server = createServer(
    createRequestHandler({
      async aggregateWeeklyMetric() {
        return {};
      },
      async post() {},
      async renderDashboard() {
        return '';
      },
      async snapshot() {
        return {};
      },
      async syncExternalData(request) {
        invocationCount += 1;
        receivedRequest = request;
        return { instanceCount: 1 };
      },
      triggerToken: 'trigger-token',
    }),
  );
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const { port } = server.address() as AddressInfo;
    const base = `http://127.0.0.1:${port}/sync-external-data`;
    const methodResponse = await fetch(base);
    assert.equal(methodResponse.status, 405);
    assert.equal(methodResponse.headers.get('allow'), 'POST');
    const missingBodyResponse = await fetch(base, {
      headers: { 'Content-Type': 'application/json' },
      method: 'POST',
    });
    assert.equal(missingBodyResponse.status, 400);
    const response = await fetch(base, {
      body: JSON.stringify({
        raw_metric_period: { end: '2026-08-31', start: '2026-08-24' },
        raw_metric_type: 'sheep-council-meetings',
      }),
      headers: { 'Content-Type': 'application/json' },
      method: 'POST',
    });
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { instanceCount: 1 });
    assert.deepEqual(receivedRequest, {
      raw_metric_period: { end: '2026-08-31', start: '2026-08-24' },
      raw_metric_type: 'sheep-council-meetings',
    });
    assert.equal(invocationCount, 1);
  } finally {
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
  }
});

test('aggregate endpoint requires authentication', async () => {
  const invocations: string[] = [];
  const server = createServer(
    createRequestHandler({
      async aggregateWeeklyMetric(period) {
        invocations.push(`aggregate:${period.start}:${period.end}`);
        return { weekStartDate: period.start };
      },
      async post() {},
      async renderDashboard() {
        return '';
      },
      async snapshot() {
        return {};
      },
      async syncExternalData() {
        return {};
      },
      triggerToken: 'trigger-token',
    }),
  );
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const { port } = server.address() as AddressInfo;
    const base = `http://127.0.0.1:${port}`;
    const unauthorized = await fetch(`${base}/aggregate-weekly-metric`, {
      method: 'POST',
    });
    assert.equal(unauthorized.status, 401);
    const aggregate = await fetch(`${base}/aggregate-weekly-metric`, {
      body: JSON.stringify({ end: '2026-08-24', start: '2026-08-17' }),
      headers: {
        Authorization: 'Bearer trigger-token',
        'Content-Type': 'application/json',
      },
      method: 'POST',
    });
    assert.equal(aggregate.status, 200);
    assert.deepEqual(await aggregate.json(), { weekStartDate: '2026-08-17' });
    assert.deepEqual(invocations, ['aggregate:2026-08-17:2026-08-24']);
  } finally {
    await new Promise<void>((resolve, reject) =>
      server.close((error) => (error ? reject(error) : resolve())),
    );
  }
});
