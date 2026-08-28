import type { PrismaClient } from './generated/prisma/client.js';
import {
  type DiscordCommunityConfig,
  loadDiscordCommunityMetrics,
} from './clients/discord.js';
import {
  type GithubIssuesConfig,
  loadDistinctGithubIssueAuthors,
} from './clients/github.js';
import {
  type LumaConfig,
  loadLatestLumaEventAttendance,
} from './clients/luma.js';
import type { WeeklyPeriod } from './report.js';

const dayMs = 24 * 60 * 60 * 1000;

export interface DailySnapshotConfig {
  discord: DiscordCommunityConfig;
  github: GithubIssuesConfig;
  luma: LumaConfig;
  timeZone: string;
}

export interface DailySnapshotResult {
  discordTotalUserCount: number;
  githubIssuesDistinctUserCount: number;
  lumaEapEventId: string;
  lumaEapJoinedCount: number;
  lumaEapSignupCount: number;
  recordedAt: Date;
  sheepCouncilActiveCount: number | null;
  sheepCouncilUserCount: number;
  sheepCouncilZoomAttendanceCount: number | null;
  sheepCouncilZoomMeetingId: string | null;
  weekStartDate: Date;
}

function dateParts(now: Date, timeZone: string) {
  const parts = new Intl.DateTimeFormat('en-US', {
    day: '2-digit',
    month: '2-digit',
    timeZone,
    weekday: 'short',
    year: 'numeric',
  }).formatToParts(now);
  const value = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find((part) => part.type === type)?.value;
  const year = Number(value('year'));
  const month = Number(value('month'));
  const day = Number(value('day'));
  const weekday = value('weekday');
  if (!year || !month || !day || !weekday) {
    throw new Error('Could not calculate the daily snapshot period');
  }
  return { day, month, weekday, year };
}

function isoDate(timestamp: number): string {
  return new Date(timestamp).toISOString().slice(0, 10);
}

function shiftDate(date: string, days: number): string {
  return isoDate(new Date(`${date}T00:00:00Z`).getTime() + days * dayMs);
}

export function currentWeeklySnapshotPeriod(
  now: Date,
  timeZone: string,
): WeeklyPeriod {
  const local = dateParts(now, timeZone);
  const weekdayIndex = [
    'Sun',
    'Mon',
    'Tue',
    'Wed',
    'Thu',
    'Fri',
    'Sat',
  ].indexOf(local.weekday);
  if (weekdayIndex < 0) throw new Error(`Unknown weekday: ${local.weekday}`);
  const localDate = Date.UTC(local.year, local.month - 1, local.day);
  const start = isoDate(
    localDate - ((weekdayIndex + 6) % 7) * dayMs,
  );
  return {
    end: shiftDate(start, 7),
    previousStart: shiftDate(start, -7),
    start,
  };
}

function timeZoneOffset(instant: Date, timeZone: string): number {
  const parts = new Intl.DateTimeFormat('en-US', {
    day: '2-digit',
    hour: '2-digit',
    hourCycle: 'h23',
    minute: '2-digit',
    month: '2-digit',
    second: '2-digit',
    timeZone,
    year: 'numeric',
  }).formatToParts(instant);
  const value = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find((part) => part.type === type)?.value);
  return (
    Date.UTC(
      value('year'),
      value('month') - 1,
      value('day'),
      value('hour'),
      value('minute'),
      value('second'),
    ) - Math.floor(instant.getTime() / 1000) * 1000
  );
}

export function startOfDayInTimeZone(
  date: string,
  timeZone: string,
): Date {
  const localMidnight = new Date(`${date}T00:00:00Z`).getTime();
  let timestamp = localMidnight;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    timestamp = localMidnight - timeZoneOffset(new Date(timestamp), timeZone);
  }
  return new Date(timestamp);
}

export async function recordDailyMetricsSnapshot(
  config: DailySnapshotConfig,
  prisma: PrismaClient,
  now = new Date(),
  fetchImpl: typeof fetch = fetch,
): Promise<DailySnapshotResult> {
  const period = currentWeeklySnapshotPeriod(now, config.timeZone);
  const [discord, luma, githubIssuesDistinctUserCount] = await Promise.all([
    loadDiscordCommunityMetrics(config.discord, fetchImpl, now),
    loadLatestLumaEventAttendance(
      config.luma,
      period,
      config.timeZone,
      fetchImpl,
    ),
    loadDistinctGithubIssueAuthors(
      config.github,
      startOfDayInTimeZone(period.start, config.timeZone),
      startOfDayInTimeZone(period.end, config.timeZone),
      fetchImpl,
    ),
  ]);
  const data: DailySnapshotResult = {
    discordTotalUserCount: discord.totalMemberCount,
    githubIssuesDistinctUserCount,
    lumaEapEventId: luma.eventId,
    lumaEapJoinedCount: luma.joinedGuests,
    lumaEapSignupCount: luma.registeredGuests,
    recordedAt: now,
    sheepCouncilActiveCount: null,
    sheepCouncilUserCount: discord.sheepCouncilMemberCount,
    sheepCouncilZoomAttendanceCount: null,
    sheepCouncilZoomMeetingId: null,
    weekStartDate: startOfDayInTimeZone(period.start, config.timeZone),
  };
  await prisma.plgWeeklyMetric.upsert({
    where: { weekStartDate: data.weekStartDate },
    create: {
      githubIssuesDistinctUserCount: data.githubIssuesDistinctUserCount,
      lumaEapZoomAttendanceCount: data.lumaEapJoinedCount,
      lumaEapSignupCount: data.lumaEapSignupCount,
      recordedAt: data.recordedAt,
      sheepCouncilActiveCount: data.sheepCouncilActiveCount,
      sheepCouncilDiscordUserCount: data.sheepCouncilUserCount,
      sheepCouncilZoomAttendanceCount:
        data.sheepCouncilZoomAttendanceCount,
      totalDiscordUserCount: data.discordTotalUserCount,
      weekStartDate: data.weekStartDate,
    },
    update: {
      githubIssuesDistinctUserCount: data.githubIssuesDistinctUserCount,
      lumaEapZoomAttendanceCount: data.lumaEapJoinedCount,
      lumaEapSignupCount: data.lumaEapSignupCount,
      recordedAt: data.recordedAt,
      sheepCouncilActiveCount: data.sheepCouncilActiveCount,
      sheepCouncilDiscordUserCount: data.sheepCouncilUserCount,
      sheepCouncilZoomAttendanceCount:
        data.sheepCouncilZoomAttendanceCount,
      totalDiscordUserCount: data.discordTotalUserCount,
    },
  });
  return data;
}
