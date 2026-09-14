import { timingSafeEqual } from 'node:crypto';
import type { IncomingMessage, ServerResponse } from 'node:http';
import {
  type ExternalDataSyncRequest,
  parseExternalDataSyncRequest,
} from './external-data.js';
import {
  parseWeeklyMetricPeriod,
  type WeeklyMetricPeriod,
} from './jobs/aggregate-weekly-metric.js';

interface RequestHandlerOptions {
  aggregateWeeklyMetric: (period: WeeklyMetricPeriod) => Promise<unknown>;
  post: () => Promise<void>;
  renderDashboard: () => Promise<string> | string;
  snapshot: () => Promise<unknown>;
  syncExternalData: (request: ExternalDataSyncRequest) => Promise<unknown>;
  triggerToken: string;
}

const maximumRequestBodyBytes = 16 * 1024;

function send(
  response: ServerResponse,
  statusCode: number,
  body = '',
  headers: Record<string, string> = {},
): void {
  response.writeHead(statusCode, {
    'Content-Length': Buffer.byteLength(body),
    'Content-Security-Policy':
      "default-src 'none'; frame-src https://us.posthog.com; script-src https://cdn.plot.ly 'unsafe-inline'; style-src 'unsafe-inline'",
    'Content-Type': 'text/plain; charset=utf-8',
    'Referrer-Policy': 'no-referrer',
    'X-Content-Type-Options': 'nosniff',
    ...headers,
  });
  response.end(body);
}

function hasValidBearerToken(
  authorization: string | undefined,
  expectedToken: string,
): boolean {
  if (!authorization?.startsWith('Bearer ')) return false;
  const actual = Buffer.from(authorization.slice('Bearer '.length));
  const expected = Buffer.from(expectedToken);
  return actual.length === expected.length && timingSafeEqual(actual, expected);
}

async function jsonRequestBody(request: IncomingMessage): Promise<unknown> {
  const contentType = request.headers['content-type']
    ?.split(';')[0]
    ?.trim()
    .toLowerCase();
  if (contentType !== 'application/json') {
    throw new Error('Content-Type must be application/json');
  }
  const chunks: Buffer[] = [];
  let byteCount = 0;
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    byteCount += buffer.length;
    if (byteCount > maximumRequestBodyBytes) {
      throw new Error('request body is too large');
    }
    chunks.push(buffer);
  }
  const body = Buffer.concat(chunks).toString('utf8');
  if (!body) throw new Error('request body is required');
  let value: unknown;
  try {
    value = JSON.parse(body);
  } catch {
    throw new Error('request body must be valid JSON');
  }
  return value;
}

async function externalDataSyncRequest(
  request: IncomingMessage,
): Promise<ExternalDataSyncRequest> {
  return parseExternalDataSyncRequest(await jsonRequestBody(request));
}

export function createRequestHandler({
  aggregateWeeklyMetric,
  post,
  renderDashboard,
  snapshot,
  syncExternalData,
  triggerToken,
}: RequestHandlerOptions) {
  return async (request: IncomingMessage, response: ServerResponse) => {
    try {
      const url = new URL(request.url ?? '/', 'http://localhost');
      if (request.method === 'GET' && url.pathname === '/') {
        try {
          send(response, 200, await renderDashboard(), {
            'Content-Type': 'text/html; charset=utf-8',
          });
        } catch (error) {
          console.error(error);
          send(response, 502, 'Product metrics unavailable');
        }
        return;
      }
      if (request.method === 'GET' && url.pathname === '/healthz') {
        send(response, 200, 'ok');
        return;
      }
      if (url.pathname === '/snapshot' && request.method !== 'POST') {
        send(response, 405, 'method not allowed', { Allow: 'POST' });
        return;
      }
      if (request.method === 'POST' && url.pathname === '/snapshot') {
        if (!hasValidBearerToken(request.headers.authorization, triggerToken)) {
          send(response, 401, 'unauthorized', {
            'WWW-Authenticate': 'Bearer',
          });
          return;
        }
        try {
          send(response, 200, JSON.stringify(await snapshot()), {
            'Content-Type': 'application/json; charset=utf-8',
          });
        } catch (error) {
          console.error(error);
          send(response, 502, 'Metrics snapshot failed');
        }
        return;
      }
      if (url.pathname === '/sync-external-data' && request.method !== 'POST') {
        send(response, 405, 'method not allowed', { Allow: 'POST' });
        return;
      }
      if (request.method === 'POST' && url.pathname === '/sync-external-data') {
        let syncRequest: ExternalDataSyncRequest;
        try {
          syncRequest = await externalDataSyncRequest(request);
        } catch (error) {
          send(
            response,
            400,
            `Invalid external data sync request: ${error instanceof Error ? error.message : 'unknown error'}`,
          );
          return;
        }
        try {
          send(
            response,
            200,
            JSON.stringify(await syncExternalData(syncRequest)),
            {
              'Content-Type': 'application/json; charset=utf-8',
            },
          );
        } catch (error) {
          console.error(error);
          send(response, 502, 'External data sync failed');
        }
        return;
      }
      if (
        url.pathname === '/aggregate-weekly-metric' &&
        request.method !== 'POST'
      ) {
        send(response, 405, 'method not allowed', { Allow: 'POST' });
        return;
      }
      if (
        request.method === 'POST' &&
        url.pathname === '/aggregate-weekly-metric'
      ) {
        if (!hasValidBearerToken(request.headers.authorization, triggerToken)) {
          send(response, 401, 'unauthorized', {
            'WWW-Authenticate': 'Bearer',
          });
          return;
        }
        let period: WeeklyMetricPeriod;
        try {
          period = parseWeeklyMetricPeriod(await jsonRequestBody(request));
        } catch (error) {
          send(
            response,
            400,
            `Invalid weekly metric period: ${error instanceof Error ? error.message : 'unknown error'}`,
          );
          return;
        }
        try {
          send(
            response,
            200,
            JSON.stringify(await aggregateWeeklyMetric(period)),
            {
              'Content-Type': 'application/json; charset=utf-8',
            },
          );
        } catch (error) {
          console.error(error);
          send(response, 502, 'Weekly metric aggregation failed');
        }
        return;
      }
      if (url.pathname === '/post' && request.method !== 'POST') {
        send(response, 405, 'method not allowed', { Allow: 'POST' });
        return;
      }
      if (request.method === 'POST' && url.pathname === '/post') {
        if (!hasValidBearerToken(request.headers.authorization, triggerToken)) {
          send(response, 401, 'unauthorized', {
            'WWW-Authenticate': 'Bearer',
          });
          return;
        }
        try {
          await post();
          send(response, 204);
        } catch (error) {
          console.error(error);
          send(response, 502, 'Slack post failed');
        }
        return;
      }
      send(response, 404, 'not found');
    } catch (error) {
      console.error(error);
      send(response, 400, 'bad request');
    }
  };
}
