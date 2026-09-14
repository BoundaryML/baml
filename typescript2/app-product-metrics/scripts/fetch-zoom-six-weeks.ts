import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

const zoomMeetingIdSheepCouncil = '89180085282';
const zoomApiBase = 'https://api.zoom.us/v2';
const lumaApiBase = 'https://public-api.luma.com';
const windowMs = 42 * 24 * 60 * 60 * 1000;
const outputRoot = process.env.ZOOM_PARTICIPANT_OUTPUT_DIR;
if (!outputRoot) throw new Error('ZOOM_PARTICIPANT_OUTPUT_DIR is required');

function required(name: string): string {
  const value = process.env[name];
  if (!value || value === 'REPLACE_ME') throw new Error(`${name} is required`);
  return value;
}

const accountId = required('ZOOM_OPSBOT_ACCOUNT_ID');
const clientId = required('ZOOM_OPSBOT_CLIENT_ID');
const clientSecret = required('ZOOM_OPSBOT_CLIENT_SECRET');
const lumaApiKey = required('LUMA_API_KEY');
const retrievedAt = new Date();
const windowStart = new Date(retrievedAt.getTime() - windowMs);

async function jsonFile(relativePath: string, value: unknown): Promise<string> {
  const target = path.join(outputRoot, relativePath);
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(value, null, 2)}\n`, 'utf8');
  return relativePath;
}

const tokenResponse = await fetch(
  `https://zoom.us/oauth/token?grant_type=account_credentials&account_id=${encodeURIComponent(accountId)}`,
  {
    headers: {
      Authorization: `Basic ${Buffer.from(`${clientId}:${clientSecret}`).toString('base64')}`,
    },
    method: 'POST',
    signal: AbortSignal.timeout(30_000),
  },
);
const tokenBody = (await tokenResponse.json()) as Record<string, unknown>;
if (!tokenResponse.ok || typeof tokenBody.access_token !== 'string') {
  throw new Error(`Zoom token request failed: ${tokenResponse.status}`);
}
const accessToken = tokenBody.access_token;

async function zoom(pathname: string): Promise<unknown> {
  const response = await fetch(`${zoomApiBase}${pathname}`, {
    headers: { Authorization: `Bearer ${accessToken}` },
    signal: AbortSignal.timeout(30_000),
  });
  const body = (await response.json()) as Record<string, unknown>;
  if (!response.ok) {
    throw new Error(
      `Zoom request failed for ${pathname.split('?')[0]}: ${response.status} ${String(body.message ?? body.code ?? '')}`,
    );
  }
  return body;
}

interface LumaEvent {
  end_at: string;
  id: string;
  meeting_url: string;
  name: string;
  start_at: string;
}

async function lumaEvents(): Promise<{
  events: LumaEvent[];
  requests: string[];
}> {
  const events: LumaEvent[] = [];
  const requests: string[] = [];
  let cursor: string | undefined;
  do {
    const parameters = new URLSearchParams({
      access: 'manage',
      after: new Date(
        windowStart.getTime() - 24 * 60 * 60 * 1000,
      ).toISOString(),
      before: new Date(
        retrievedAt.getTime() + 24 * 60 * 60 * 1000,
      ).toISOString(),
      pagination_limit: '100',
      sort_column: 'start_at',
      sort_direction: 'asc',
    });
    if (cursor) parameters.set('pagination_cursor', cursor);
    const pathname = `/v1/calendars/events/list?${parameters}`;
    requests.push(pathname);
    const response = await fetch(`${lumaApiBase}${pathname}`, {
      headers: { 'x-luma-api-key': lumaApiKey },
      signal: AbortSignal.timeout(30_000),
    });
    const body = (await response.json()) as Record<string, unknown>;
    if (!response.ok || !Array.isArray(body.entries)) {
      throw new Error(`Luma event request failed: ${response.status}`);
    }
    for (const candidate of body.entries) {
      const event = candidate as Partial<LumaEvent> & {
        location_type?: unknown;
      };
      if (
        event.name !== 'BAML Early Access' ||
        event.location_type !== 'zoom' ||
        typeof event.id !== 'string' ||
        typeof event.start_at !== 'string' ||
        typeof event.end_at !== 'string' ||
        typeof event.meeting_url !== 'string'
      ) {
        continue;
      }
      const start = new Date(event.start_at);
      if (start >= windowStart && start <= retrievedAt) {
        events.push(event as LumaEvent);
      }
    }
    cursor = body.has_more ? String(body.next_cursor) : undefined;
  } while (cursor);
  return { events, requests };
}

function meetingId(meetingUrl: string): string {
  const match = new URL(meetingUrl).pathname.match(/^\/j\/(\d+)$/);
  if (!match?.[1]) throw new Error('Could not extract Zoom meeting ID');
  return match[1];
}

interface ZoomInstance {
  start_time: string;
  uuid: string;
}

function zoomInstances(value: unknown): ZoomInstance[] {
  const body = value as { meetings?: unknown };
  if (!Array.isArray(body.meetings)) {
    throw new Error('Zoom returned an invalid meeting instances response');
  }
  return body.meetings.map((candidate) => {
    const instance = candidate as Partial<ZoomInstance>;
    if (
      typeof instance.uuid !== 'string' ||
      typeof instance.start_time !== 'string'
    ) {
      throw new Error('Zoom returned an invalid meeting instance');
    }
    return instance as ZoomInstance;
  });
}

function encodedUuid(uuid: string): string {
  return encodeURIComponent(encodeURIComponent(uuid));
}

interface ParticipantCapture {
  endpoint: string;
  files: string[];
}

async function participants(
  directory: string,
  uuid: string,
  kind: 'past' | 'report',
): Promise<ParticipantCapture> {
  const endpoint =
    kind === 'past'
      ? `/past_meetings/${encodedUuid(uuid)}/participants`
      : `/report/meetings/${encodedUuid(uuid)}/participants`;
  const files: string[] = [];
  let nextPageToken = '';
  let pageNumber = 1;
  do {
    const parameters = new URLSearchParams({ page_size: '300' });
    if (nextPageToken) parameters.set('next_page_token', nextPageToken);
    const response = (await zoom(`${endpoint}?${parameters}`)) as {
      next_page_token?: unknown;
    };
    files.push(
      await jsonFile(
        `${directory}/${kind}-participants-page-${String(pageNumber).padStart(3, '0')}.json`,
        response,
      ),
    );
    nextPageToken =
      typeof response.next_page_token === 'string'
        ? response.next_page_token
        : '';
    pageNumber += 1;
  } while (nextPageToken);
  return { endpoint, files };
}

interface MeetingCapture {
  category: 'eap' | 'sheep-council';
  instanceStartTime: string;
  instanceUuid: string;
  lumaEventId?: string;
  lumaEventStartTime?: string;
  participantResponses: ParticipantCapture[];
  zoomMeetingId: string;
}

async function captureMeeting(
  category: MeetingCapture['category'],
  id: string,
  instance: ZoomInstance,
  directory: string,
  lumaEvent?: LumaEvent,
): Promise<MeetingCapture> {
  return {
    category,
    instanceStartTime: instance.start_time,
    instanceUuid: instance.uuid,
    ...(lumaEvent
      ? { lumaEventId: lumaEvent.id, lumaEventStartTime: lumaEvent.start_at }
      : {}),
    participantResponses: await Promise.all([
      participants(directory, instance.uuid, 'past'),
      participants(directory, instance.uuid, 'report'),
    ]),
    zoomMeetingId: id,
  };
}

const captures: MeetingCapture[] = [];
const unavailableEapEvents: Array<{
  lumaEventId: string;
  lumaEventStartTime: string;
  reason: string;
  zoomMeetingId: string;
}> = [];
const instanceResponses: Array<{
  endpoint: string;
  file: string;
  zoomMeetingId: string;
}> = [];

const sheepInstancesEndpoint = `/past_meetings/${zoomMeetingIdSheepCouncil}/instances`;
const sheepInstancesBody = await zoom(sheepInstancesEndpoint);
instanceResponses.push({
  endpoint: sheepInstancesEndpoint,
  file: await jsonFile(
    `sheep-council/meeting-${zoomMeetingIdSheepCouncil}-instances.json`,
    sheepInstancesBody,
  ),
  zoomMeetingId: zoomMeetingIdSheepCouncil,
});
for (const instance of zoomInstances(sheepInstancesBody).filter((candidate) => {
  const start = new Date(candidate.start_time);
  return start >= windowStart && start <= retrievedAt;
})) {
  const directory = `sheep-council/${instance.start_time.slice(0, 10)}-${instance.uuid.replaceAll(/[^A-Za-z0-9_-]/g, '_')}`;
  captures.push(
    await captureMeeting(
      'sheep-council',
      zoomMeetingIdSheepCouncil,
      instance,
      directory,
    ),
  );
}

const luma = await lumaEvents();
for (const event of luma.events) {
  const id = meetingId(event.meeting_url);
  const endpoint = `/past_meetings/${id}/instances`;
  const instancesBody = await zoom(endpoint);
  const instances = zoomInstances(instancesBody);
  const directory = `eap/${event.start_at.slice(0, 10)}-${event.id}`;
  instanceResponses.push({
    endpoint,
    file: await jsonFile(
      `${directory}/meeting-${id}-instances.json`,
      instancesBody,
    ),
    zoomMeetingId: id,
  });
  const eventTimestamp = new Date(event.start_at).getTime();
  const instance = instances.sort(
    (left, right) =>
      Math.abs(new Date(left.start_time).getTime() - eventTimestamp) -
      Math.abs(new Date(right.start_time).getTime() - eventTimestamp),
  )[0];
  if (!instance) {
    unavailableEapEvents.push({
      lumaEventId: event.id,
      lumaEventStartTime: event.start_at,
      reason: 'Zoom returned no past meeting instances',
      zoomMeetingId: id,
    });
    continue;
  }
  const difference = Math.abs(
    new Date(instance.start_time).getTime() - eventTimestamp,
  );
  if (difference > 24 * 60 * 60 * 1000) {
    throw new Error(`No matching Zoom instance found for ${event.id}`);
  }
  captures.push(await captureMeeting('eap', id, instance, directory, event));
}

const manifest = {
  captures,
  instanceResponses,
  lumaDiscovery: {
    apiBase: lumaApiBase,
    eventCount: luma.events.length,
    events: luma.events.map((event) => ({
      endAt: event.end_at,
      id: event.id,
      startAt: event.start_at,
      zoomMeetingId: meetingId(event.meeting_url),
    })),
    requests: luma.requests,
  },
  retrievedAt: retrievedAt.toISOString(),
  unavailableEapEvents,
  window: {
    end: retrievedAt.toISOString(),
    start: windowStart.toISOString(),
    type: 'rolling-42-days',
  },
  zoom: {
    apiBase: zoomApiBase,
    grantedScopes: String(tokenBody.scope ?? '')
      .split(' ')
      .filter(Boolean)
      .sort(),
    sheepCouncilRecurringMeetingId: zoomMeetingIdSheepCouncil,
  },
};
await jsonFile('manifest.json', manifest);
console.log(
  JSON.stringify({
    captureCount: captures.length,
    eapCaptureCount: captures.filter((capture) => capture.category === 'eap')
      .length,
    outputRoot,
    retrievedAt: retrievedAt.toISOString(),
    sheepCouncilCaptureCount: captures.filter(
      (capture) => capture.category === 'sheep-council',
    ).length,
    windowStart: windowStart.toISOString(),
  }),
);
