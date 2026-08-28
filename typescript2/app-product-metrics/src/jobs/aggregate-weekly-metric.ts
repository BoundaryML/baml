import {
  type DiscordCommunityConfig,
  loadDiscordCommunityMetrics,
} from '../clients/discord.js';
import { type PostHogConfig, runHogQl } from '../clients/posthog.js';
import type { PrismaClient } from '../generated/prisma/client.js';
import { PlgWeeklyMetricRawType } from '../generated/prisma/enums.js';
import { startOfDayInTimeZone } from '../snapshot.js';

const dayMs = 24 * 60 * 60 * 1000;
const minimumAttendanceSeconds = 10 * 60;

export interface WeeklyMetricPeriod {
  end: string;
  start: string;
}

export interface WeeklyMetricAggregationConfig {
  discord: DiscordCommunityConfig;
  posthog: PostHogConfig;
  timeZone: string;
}

export interface WeeklyMetricAggregationResult {
  githubIssuesDistinctUserCount: number;
  lumaEapSignupCount: number;
  lumaEapZoomAttendanceCount: number;
  recordedAt: Date;
  sheepCouncilActiveCount: null;
  sheepCouncilDiscordUserCount: number;
  sheepCouncilZoomAttendanceCount: number;
  totalDiscordUserCount: number;
  weekStartDate: Date;
}

function record(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value as Record<string, unknown>;
}

function array(value: unknown, name: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${name} must be an array`);
  return value;
}

function string(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value.trim()) {
    throw new Error(`${name} must be a non-empty string`);
  }
  return value;
}

function isoDate(value: unknown, name: string): { date: Date; value: string } {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    throw new Error(`${name} must use YYYY-MM-DD`);
  }
  const date = new Date(`${value}T00:00:00Z`);
  if (
    !Number.isFinite(date.getTime()) ||
    date.toISOString().slice(0, 10) !== value
  ) {
    throw new Error(`${name} must be a valid calendar date`);
  }
  return { date, value };
}

export function parseWeeklyMetricPeriod(value: unknown): WeeklyMetricPeriod {
  const period = record(value, 'request body');
  const keys = Object.keys(period).sort();
  if (keys.length !== 2 || keys[0] !== 'end' || keys[1] !== 'start') {
    throw new Error('request body must contain exactly end, start');
  }
  const start = isoDate(period.start, 'start');
  const end = isoDate(period.end, 'end');
  if (start.date.getUTCDay() !== 1 || end.date.getUTCDay() !== 1) {
    throw new Error('start and end must be Mondays');
  }
  if (end.date.getTime() - start.date.getTime() !== 7 * dayMs) {
    throw new Error('start and end must span exactly one week');
  }
  return { end: end.value, start: start.value };
}

function participantIdentity(
  participant: Record<string, unknown>,
  name: string,
): string {
  for (const field of ['id', 'user_email', 'name']) {
    const value = participant[field];
    if (typeof value === 'string' && value.trim()) {
      return `${field}:${value.trim().toLowerCase()}`;
    }
  }
  throw new Error(`${name} has no participant identity`);
}

function zoomAttendanceCount(
  captures: unknown[],
  name: string,
  zoomCapture: (
    capture: Record<string, unknown>,
  ) => Record<string, unknown> | null,
): number {
  let count = 0;
  for (const [captureIndex, captureValue] of captures.entries()) {
    const captureName = `${name}.captures[${captureIndex}]`;
    const capture = record(captureValue, captureName);
    const zoom = zoomCapture(capture);
    if (!zoom) continue;
    const instance = record(zoom.instance, `${captureName}.instance`);
    const occurrenceId = string(instance.uuid, `${captureName}.instance.uuid`);
    const participants = record(
      zoom.participants,
      `${captureName}.participants`,
    );
    const past = record(participants.past, `${captureName}.participants.past`);
    const durations = new Map<string, number>();
    for (const [pageIndex, pageValue] of array(
      past.pages,
      `${captureName}.participants.past.pages`,
    ).entries()) {
      const pageName = `${captureName}.participants.past.pages[${pageIndex}]`;
      const page = record(pageValue, pageName);
      const response = record(page.response, `${pageName}.response`);
      for (const [participantIndex, participantValue] of array(
        response.participants,
        `${pageName}.response.participants`,
      ).entries()) {
        const participantName = `${pageName}.response.participants[${participantIndex}]`;
        const participant = record(participantValue, participantName);
        if (typeof participant.internal_user !== 'boolean') {
          throw new Error(`${participantName}.internal_user must be a boolean`);
        }
        const duration = participant.duration;
        if (!Number.isSafeInteger(duration) || Number(duration) < 0) {
          throw new Error(
            `${participantName}.duration must be a non-negative integer`,
          );
        }
        if (participant.internal_user) continue;
        const identity = `${occurrenceId}:${participantIdentity(participant, participantName)}`;
        durations.set(
          identity,
          (durations.get(identity) ?? 0) + Number(duration),
        );
      }
    }
    count += [...durations.values()].filter(
      (duration) => duration >= minimumAttendanceSeconds,
    ).length;
  }
  return count;
}

export function sheepCouncilAttendanceCount(rawMetricData: unknown): number {
  const raw = record(rawMetricData, 'SHEEP_COUNCIL_MEETINGS rawMetricData');
  const zoom = record(raw.zoom, 'SHEEP_COUNCIL_MEETINGS rawMetricData.zoom');
  return zoomAttendanceCount(
    array(zoom.captures, 'SHEEP_COUNCIL_MEETINGS rawMetricData.zoom.captures'),
    'SHEEP_COUNCIL_MEETINGS rawMetricData.zoom',
    (capture) => capture,
  );
}

function eapZoomCapture(
  capture: Record<string, unknown>,
): Record<string, unknown> | null {
  const zoom = record(capture.zoom, 'EAP_MEETINGS capture.zoom');
  if (zoom.resolution === 'not-found') return null;
  if (zoom.resolution !== 'resolved') {
    throw new Error('EAP_MEETINGS capture.zoom.resolution is invalid');
  }
  return zoom;
}

export function lumaEapAttendanceCount(rawMetricData: unknown): number {
  const raw = record(rawMetricData, 'EAP_MEETINGS rawMetricData');
  return zoomAttendanceCount(
    array(raw.captures, 'EAP_MEETINGS rawMetricData.captures'),
    'EAP_MEETINGS rawMetricData',
    eapZoomCapture,
  );
}

export function lumaEapSignupCount(rawMetricData: unknown): number {
  const raw = record(rawMetricData, 'EAP_MEETINGS rawMetricData');
  let count = 0;
  for (const [captureIndex, captureValue] of array(
    raw.captures,
    'EAP_MEETINGS rawMetricData.captures',
  ).entries()) {
    const captureName = `EAP_MEETINGS rawMetricData.captures[${captureIndex}]`;
    const capture = record(captureValue, captureName);
    const event = record(capture.event, `${captureName}.event`);
    const eventId = string(event.id, `${captureName}.event.id`);
    const guests = new Set<string>();
    for (const [pageIndex, pageValue] of array(
      capture.guests,
      `${captureName}.guests`,
    ).entries()) {
      const pageName = `${captureName}.guests[${pageIndex}]`;
      const page = record(pageValue, pageName);
      const response = record(page.response, `${pageName}.response`);
      for (const [guestIndex, guestValue] of array(
        response.entries,
        `${pageName}.response.entries`,
      ).entries()) {
        const guestName = `${pageName}.response.entries[${guestIndex}]`;
        const guest = record(guestValue, guestName);
        if (guest.approval_status !== 'approved') {
          throw new Error(`${guestName}.approval_status must be approved`);
        }
        guests.add(`${eventId}:${string(guest.id, `${guestName}.id`)}`);
      }
    }
    count += guests.size;
  }
  return count;
}

export function githubIssuesDistinctUsersQuery(
  period: WeeklyMetricPeriod,
  timeZone: string,
): string {
  if (!/^[A-Za-z_]+\/[A-Za-z_]+$/.test(timeZone)) {
    throw new Error('timeZone must be an IANA area/location name');
  }
  return `SELECT count(DISTINCT JSONExtractString(user, 'id')) AS distinct_users
FROM github_boundaryml_baml__issues
WHERE toDateTime(created_at) >= toDateTime('${period.start} 00:00:00', '${timeZone}')
  AND toDateTime(created_at) < toDateTime('${period.end} 00:00:00', '${timeZone}')`;
}

export async function loadGithubIssuesDistinctUserCount(
  config: PostHogConfig,
  period: WeeklyMetricPeriod,
  timeZone: string,
  fetchImpl: typeof fetch,
): Promise<number> {
  const result = await runHogQl(
    config,
    `weekly_github_issue_users_${period.start}_${period.end}`,
    githubIssuesDistinctUsersQuery(period, timeZone),
    fetchImpl,
  );
  if (
    result.columns.length !== 1 ||
    result.columns[0] !== 'distinct_users' ||
    result.results.length !== 1
  ) {
    throw new Error('PostHog GitHub issues query schema changed');
  }
  const value = Number(result.results[0]?.[0]);
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(
      'PostHog returned an invalid distinct GitHub issue user count',
    );
  }
  return value;
}

async function latestRawMetric(
  prisma: PrismaClient,
  weekStartDate: Date,
  rawMetricType: PlgWeeklyMetricRawType,
) {
  const row = await prisma.plgWeeklyMetricRaw.findFirst({
    orderBy: { recordedAt: 'desc' },
    select: { rawMetricData: true },
    where: { rawMetricType, weekStartDate },
  });
  if (!row) {
    throw new Error(`No ${rawMetricType} raw metric exists for this week`);
  }
  return row.rawMetricData;
}

export async function aggregateWeeklyMetric(
  config: WeeklyMetricAggregationConfig,
  prisma: PrismaClient,
  period: WeeklyMetricPeriod,
  now = new Date(),
  fetchImpl: typeof fetch = fetch,
): Promise<WeeklyMetricAggregationResult> {
  const validatedPeriod = parseWeeklyMetricPeriod(period);
  const weekStartDate = startOfDayInTimeZone(
    validatedPeriod.start,
    config.timeZone,
  );
  const [discord, sheepCouncilRaw, eapRaw, githubCount] = await Promise.all([
    loadDiscordCommunityMetrics(config.discord, fetchImpl, now),
    latestRawMetric(
      prisma,
      weekStartDate,
      PlgWeeklyMetricRawType.SHEEP_COUNCIL_MEETINGS,
    ),
    latestRawMetric(prisma, weekStartDate, PlgWeeklyMetricRawType.EAP_MEETINGS),
    loadGithubIssuesDistinctUserCount(
      config.posthog,
      validatedPeriod,
      config.timeZone,
      fetchImpl,
    ),
  ]);
  const data: WeeklyMetricAggregationResult = {
    githubIssuesDistinctUserCount: githubCount,
    lumaEapSignupCount: lumaEapSignupCount(eapRaw),
    lumaEapZoomAttendanceCount: lumaEapAttendanceCount(eapRaw),
    recordedAt: now,
    sheepCouncilActiveCount: null,
    sheepCouncilDiscordUserCount: discord.sheepCouncilMemberCount,
    sheepCouncilZoomAttendanceCount:
      sheepCouncilAttendanceCount(sheepCouncilRaw),
    totalDiscordUserCount: discord.totalMemberCount,
    weekStartDate,
  };
  const { weekStartDate: rowWeekStartDate, ...values } = data;
  await prisma.plgWeeklyMetric.upsert({
    create: data,
    update: values,
    where: { weekStartDate: rowWeekStartDate },
  });
  return data;
}
