import {
  createZoomClient,
  type ZoomClient,
  type ZoomConfig,
  type ZoomMeetingInstance,
} from '../clients/zoom.js';
import type { RawMetricPeriod } from '../external-data.js';
import type { Prisma, PrismaClient } from '../generated/prisma/client.js';
import { PlgWeeklyMetricRawType } from '../generated/prisma/enums.js';
import { startOfDayInTimeZone } from '../snapshot.js';

const lumaApiBase = 'https://public-api.luma.com';
const maximumInstanceDifferenceMs = 24 * 60 * 60 * 1000;

export interface LumaEapSyncConfig {
  eventName: string;
  lumaApiKey: string;
  timeZone: string;
  zoom: ZoomConfig;
}

export interface LumaEapSyncResult {
  eventCount: number;
  recordedAt: Date;
  resolvedEventCount: number;
  weekStartDate: Date;
}

interface LumaEvent {
  endAt: string;
  id: string;
  name: string;
  startAt: string;
  zoomMeetingId: string;
}

interface ResponsePage {
  request: string;
  response: Record<string, unknown>;
}

function inputJson(value: unknown): Prisma.InputJsonValue {
  return JSON.parse(JSON.stringify(value)) as Prisma.InputJsonValue;
}

function record(value: unknown, resource: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Luma returned an invalid ${resource} response`);
  }
  return value as Record<string, unknown>;
}

function entries(
  response: Record<string, unknown>,
  resource: string,
): unknown[] {
  if (!Array.isArray(response.entries)) {
    throw new Error(`Luma returned an invalid ${resource} response`);
  }
  if (
    response.has_more !== undefined &&
    typeof response.has_more !== 'boolean'
  ) {
    throw new Error(`Luma returned an invalid ${resource} pagination state`);
  }
  if (response.has_more && typeof response.next_cursor !== 'string') {
    throw new Error(`Luma returned an invalid ${resource} cursor`);
  }
  return response.entries;
}

function meetingId(meetingUrl: string): string {
  const match = new URL(meetingUrl).pathname.match(/\/j\/(\d+)/);
  if (!match?.[1]) {
    throw new Error('Could not extract a Zoom meeting ID from the Luma event');
  }
  return match[1];
}

async function lumaGet(
  apiKey: string,
  pathname: string,
  parameters: URLSearchParams,
  fetchImpl: typeof fetch,
): Promise<Record<string, unknown>> {
  const response = await fetchImpl(`${lumaApiBase}${pathname}?${parameters}`, {
    headers: { 'x-luma-api-key': apiKey },
    signal: AbortSignal.timeout(30_000),
  });
  const body = record(await response.json(), pathname);
  if (!response.ok) {
    throw new Error(
      `Luma request failed: ${String(body.message ?? response.status)}`,
    );
  }
  return body;
}

async function listEvents(
  config: LumaEapSyncConfig,
  weekStartDate: Date,
  weekEndDate: Date,
  fetchImpl: typeof fetch,
): Promise<{ events: LumaEvent[]; requests: string[] }> {
  const events: LumaEvent[] = [];
  const requests: string[] = [];
  let cursor: string | undefined;
  do {
    const parameters = new URLSearchParams({
      access: 'manage',
      after: weekStartDate.toISOString(),
      before: weekEndDate.toISOString(),
      pagination_limit: '100',
      sort_column: 'start_at',
      sort_direction: 'asc',
    });
    if (cursor) parameters.set('pagination_cursor', cursor);
    const request = `/v1/calendars/events/list?${parameters}`;
    requests.push(request);
    const response = await lumaGet(
      config.lumaApiKey,
      '/v1/calendars/events/list',
      parameters,
      fetchImpl,
    );
    for (const candidate of entries(response, 'events list')) {
      const event = record(candidate, 'event');
      if (event.name !== config.eventName || event.location_type !== 'zoom') {
        continue;
      }
      if (
        typeof event.id !== 'string' ||
        typeof event.name !== 'string' ||
        typeof event.start_at !== 'string' ||
        typeof event.end_at !== 'string' ||
        typeof event.meeting_url !== 'string'
      ) {
        throw new Error('Luma returned an invalid EAP event');
      }
      const startAt = new Date(event.start_at);
      if (
        !Number.isFinite(startAt.getTime()) ||
        startAt < weekStartDate ||
        startAt >= weekEndDate
      ) {
        continue;
      }
      events.push({
        endAt: event.end_at,
        id: event.id,
        name: event.name,
        startAt: event.start_at,
        zoomMeetingId: meetingId(event.meeting_url),
      });
    }
    cursor = response.has_more ? String(response.next_cursor) : undefined;
  } while (cursor);
  return { events, requests };
}

async function guestPages(
  config: LumaEapSyncConfig,
  eventId: string,
  fetchImpl: typeof fetch,
): Promise<ResponsePage[]> {
  const pages: ResponsePage[] = [];
  let cursor: string | undefined;
  do {
    const parameters = new URLSearchParams({
      approval_status: 'approved',
      event_id: eventId,
      pagination_limit: '100',
    });
    if (cursor) parameters.set('pagination_cursor', cursor);
    const request = `/v1/events/guests/list?${parameters}`;
    const response = await lumaGet(
      config.lumaApiKey,
      '/v1/events/guests/list',
      parameters,
      fetchImpl,
    );
    entries(response, 'guest list');
    pages.push({ request, response });
    cursor = response.has_more ? String(response.next_cursor) : undefined;
  } while (cursor);
  return pages;
}

function matchingInstance(
  event: LumaEvent,
  instances: ZoomMeetingInstance[],
): ZoomMeetingInstance | undefined {
  const eventTimestamp = new Date(event.startAt).getTime();
  const closest = [...instances].sort(
    (left, right) =>
      Math.abs(new Date(left.startTime).getTime() - eventTimestamp) -
      Math.abs(new Date(right.startTime).getTime() - eventTimestamp),
  )[0];
  if (
    !closest ||
    Math.abs(new Date(closest.startTime).getTime() - eventTimestamp) >
      maximumInstanceDifferenceMs
  ) {
    return undefined;
  }
  return closest;
}

async function resolveEvent(
  config: LumaEapSyncConfig,
  zoom: ZoomClient,
  event: LumaEvent,
  fetchImpl: typeof fetch,
) {
  const [guests, instanceResponse] = await Promise.all([
    guestPages(config, event.id, fetchImpl),
    zoom.meetingInstances(event.zoomMeetingId),
  ]);
  const instance = matchingInstance(event, instanceResponse.instances);
  if (!instance) {
    return {
      event,
      guests,
      zoom: {
        instancesEndpoint: instanceResponse.endpoint,
        instancesResponse: instanceResponse.response,
        resolution: 'not-found',
      },
    };
  }
  const [past, report] = await Promise.all([
    zoom.participantPages(instance.uuid, 'past'),
    zoom.participantPages(instance.uuid, 'report'),
  ]);
  return {
    event,
    guests,
    zoom: {
      instance: instance.raw,
      instancesEndpoint: instanceResponse.endpoint,
      instancesResponse: instanceResponse.response,
      participants: { past, report },
      resolution: 'resolved',
    },
  };
}

export async function fetchLumaEapData(
  config: LumaEapSyncConfig,
  prisma: PrismaClient,
  period: RawMetricPeriod,
  now = new Date(),
  fetchImpl: typeof fetch = fetch,
): Promise<LumaEapSyncResult> {
  const weekStartDate = startOfDayInTimeZone(period.start, config.timeZone);
  const weekEndDate = startOfDayInTimeZone(period.end, config.timeZone);
  const [discovery, zoom] = await Promise.all([
    listEvents(config, weekStartDate, weekEndDate, fetchImpl),
    createZoomClient(config.zoom, fetchImpl),
  ]);
  const captures = await Promise.all(
    discovery.events.map((event) =>
      resolveEvent(config, zoom, event, fetchImpl),
    ),
  );
  const rawMetricData = inputJson({
    captures,
    eventName: config.eventName,
    luma: {
      eventListRequests: discovery.requests,
    },
    period: {
      end: weekEndDate.toISOString(),
      start: weekStartDate.toISOString(),
      timeZone: config.timeZone,
    },
    source: 'luma-eap',
    version: 1,
  });
  await prisma.plgWeeklyMetricRaw.create({
    data: {
      rawMetricData,
      rawMetricType: PlgWeeklyMetricRawType.EAP_MEETINGS,
      recordedAt: now,
      weekStartDate,
    },
  });
  return {
    eventCount: captures.length,
    recordedAt: now,
    resolvedEventCount: captures.filter(
      (capture) => capture.zoom.resolution === 'resolved',
    ).length,
    weekStartDate,
  };
}
