import type { WeeklyPeriod } from '../report.js';

const apiBase = 'https://public-api.luma.com';
const dayMs = 24 * 60 * 60 * 1000;

export interface LumaConfig {
  apiKey: string;
  eventName: string;
}

export interface WeeklyLumaAttendanceMetrics {
  eventCount: number;
  joinedGuests: number;
  joinRatePercent: number | null;
  period: WeeklyPeriod;
  registeredGuests: number;
}

export interface LumaEventAttendanceMetrics {
  eventId: string;
  joinedGuests: number;
  registeredGuests: number;
}

interface LumaEvent {
  endAt: string;
  id: string;
  startAt: string;
}

interface PaginatedResponse {
  entries?: unknown;
  has_more?: unknown;
  next_cursor?: unknown;
}

function shiftDate(date: string, days: number): string {
  return new Date(new Date(`${date}T00:00:00Z`).getTime() + days * dayMs)
    .toISOString()
    .slice(0, 10);
}

function localIsoDate(timestamp: string, timeZone: string): string {
  const parts = new Intl.DateTimeFormat('en-US', {
    day: '2-digit',
    month: '2-digit',
    timeZone,
    year: 'numeric',
  }).formatToParts(new Date(timestamp));
  const value = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((part) => part.type === type)?.value;
  const year = value('year');
  const month = value('month');
  const day = value('day');
  if (!year || !month || !day) {
    throw new Error('Could not calculate the Luma event date');
  }
  return `${year}-${month}-${day}`;
}

async function get(
  config: LumaConfig,
  path: string,
  parameters: URLSearchParams,
  fetchImpl: typeof fetch,
): Promise<unknown> {
  const response = await fetchImpl(`${apiBase}${path}?${parameters}`, {
    headers: { 'x-luma-api-key': config.apiKey },
    signal: AbortSignal.timeout(30_000),
  });
  const result = (await response.json()) as { message?: string };
  if (!response.ok) {
    throw new Error(
      `Luma request failed: ${result.message ?? response.status}`,
    );
  }
  return result;
}

function page(
  result: unknown,
  resource: string,
): PaginatedResponse & {
  entries: unknown[];
  has_more: boolean;
} {
  const candidate = result as PaginatedResponse;
  if (
    !Array.isArray(candidate.entries) ||
    typeof candidate.has_more !== 'boolean' ||
    (candidate.has_more && typeof candidate.next_cursor !== 'string')
  ) {
    throw new Error(`Luma ${resource} response schema changed`);
  }
  return candidate as PaginatedResponse & {
    entries: unknown[];
    has_more: boolean;
  };
}

async function listEvents(
  config: LumaConfig,
  periods: WeeklyPeriod[],
  fetchImpl: typeof fetch,
): Promise<LumaEvent[]> {
  const first = periods[0];
  const last = periods.at(-1);
  if (!first || !last) return [];
  const events: LumaEvent[] = [];
  let cursor: string | undefined;
  do {
    const parameters = new URLSearchParams({
      access: 'manage',
      after: `${shiftDate(first.start, -1)}T00:00:00Z`,
      before: `${shiftDate(last.end, 1)}T00:00:00Z`,
      pagination_limit: '100',
      sort_column: 'start_at',
      sort_direction: 'asc',
    });
    if (cursor) parameters.set('pagination_cursor', cursor);
    const result = page(
      await get(config, '/v1/calendars/events/list', parameters, fetchImpl),
      'events list',
    );
    for (const entry of result.entries) {
      const event = entry as Record<string, unknown>;
      if (event.name !== config.eventName || event.location_type !== 'zoom') {
        continue;
      }
      if (
        typeof event.id !== 'string' ||
        typeof event.start_at !== 'string' ||
        typeof event.end_at !== 'string'
      ) {
        throw new Error('Luma event response schema changed');
      }
      events.push({
        endAt: event.end_at,
        id: event.id,
        startAt: event.start_at,
      });
    }
    cursor = result.has_more ? String(result.next_cursor) : undefined;
  } while (cursor);
  return events;
}

async function loadEventAttendance(
  config: LumaConfig,
  event: LumaEvent,
  fetchImpl: typeof fetch,
): Promise<{ joinedGuests: number; registeredGuests: number }> {
  let joinedGuests = 0;
  let registeredGuests = 0;
  let cursor: string | undefined;
  do {
    const parameters = new URLSearchParams({
      approval_status: 'approved',
      event_id: event.id,
      pagination_limit: '100',
    });
    if (cursor) parameters.set('pagination_cursor', cursor);
    const result = page(
      await get(config, '/v1/events/guests/list', parameters, fetchImpl),
      'guests list',
    );
    for (const entry of result.entries) {
      const guest = entry as Record<string, unknown>;
      if (guest.approval_status !== 'approved') {
        throw new Error('Luma returned a non-approved guest');
      }
      if (guest.joined_at !== null && typeof guest.joined_at !== 'string') {
        throw new Error('Luma guest response schema changed');
      }
      registeredGuests += 1;
      if (guest.joined_at !== null) joinedGuests += 1;
    }
    cursor = result.has_more ? String(result.next_cursor) : undefined;
  } while (cursor);
  return { joinedGuests, registeredGuests };
}

export async function loadWeeklyLumaAttendance(
  config: LumaConfig,
  periods: WeeklyPeriod[],
  timeZone: string,
  fetchImpl: typeof fetch = fetch,
): Promise<WeeklyLumaAttendanceMetrics[]> {
  const metrics = periods.map((period) => ({
    eventCount: 0,
    joinedGuests: 0,
    joinRatePercent: null,
    period,
    registeredGuests: 0,
  }));
  const events = await listEvents(config, periods, fetchImpl);
  await Promise.all(
    events.map(async (event) => {
      const eventDate = localIsoDate(event.startAt, timeZone);
      const index = periods.findIndex(
        (period) => eventDate >= period.start && eventDate < period.end,
      );
      if (index < 0) return;
      const attendance = await loadEventAttendance(config, event, fetchImpl);
      const week = metrics[index];
      if (!week) throw new Error('Could not assign Luma event to a week');
      week.eventCount += 1;
      week.joinedGuests += attendance.joinedGuests;
      week.registeredGuests += attendance.registeredGuests;
    }),
  );
  return metrics.map((week) => ({
    ...week,
    joinRatePercent:
      week.registeredGuests === 0
        ? null
        : (week.joinedGuests / week.registeredGuests) * 100,
  }));
}

export async function loadLatestLumaEventAttendance(
  config: LumaConfig,
  period: WeeklyPeriod,
  timeZone: string,
  fetchImpl: typeof fetch = fetch,
): Promise<LumaEventAttendanceMetrics> {
  const events = (await listEvents(config, [period], fetchImpl))
    .filter((event) => {
      const eventDate = localIsoDate(event.startAt, timeZone);
      return eventDate >= period.start && eventDate < period.end;
    })
    .sort((left, right) => right.startAt.localeCompare(left.startAt));
  const event = events[0];
  if (!event) {
    throw new Error(
      `No ${config.eventName} Zoom event found from ${period.start} to ${period.end}`,
    );
  }
  return {
    eventId: event.id,
    ...(await loadEventAttendance(config, event, fetchImpl)),
  };
}
