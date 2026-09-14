import { type PostHogConfig, runHogQl } from '../src/clients/posthog.js';
import { database, disconnectDatabase } from '../src/database.js';
import type { PrismaClient } from '../src/generated/prisma/client.js';
import { PlgWeeklyMetricRawType } from '../src/generated/prisma/enums.js';
import {
  githubIssuesDistinctUsersQuery,
  lumaEapAttendanceCount,
  lumaEapSignupCount,
  sheepCouncilAttendanceCount,
  type WeeklyMetricPeriod,
} from '../src/jobs/aggregate-weekly-metric.js';
import { recentClosedWeeks } from '../src/report.js';
import { startOfDayInTimeZone } from '../src/snapshot.js';

const apply = process.argv.includes('--apply');
const recordedAt = new Date();
const timeZone = process.env.WEEKLY_POST_TIMEZONE ?? 'America/Los_Angeles';

function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

const posthogConfig: PostHogConfig = {
  host: process.env.POSTHOG_HOST ?? 'https://us.posthog.com',
  personalApiKey: required('POSTHOG_BOUNDARY_PRODUCT_METRICS_PERSONAL_API_KEY'),
  projectId: required('POSTHOG_BOUNDARY_PRODUCT_METRICS_PROJECT_ID'),
};

async function latestRawMetric(
  prisma: PrismaClient,
  weekStartDate: Date,
  rawMetricType: PlgWeeklyMetricRawType,
) {
  return prisma.plgWeeklyMetricRaw.findFirst({
    orderBy: { recordedAt: 'desc' },
    select: { rawMetricData: true, recordedAt: true },
    where: { rawMetricType, weekStartDate },
  });
}

async function githubCount(period: WeeklyMetricPeriod): Promise<number> {
  const result = await runHogQl(
    posthogConfig,
    `backfill_weekly_github_issue_users_${period.start}_${period.end}`,
    githubIssuesDistinctUsersQuery(period, timeZone),
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
    throw new Error('PostHog returned an invalid distinct GitHub issue count');
  }
  return value;
}

const prisma = database();
try {
  const periods = recentClosedWeeks(recordedAt, timeZone, 6);
  const rows = await Promise.all(
    periods.map(async (period) => {
      const weekStartDate = startOfDayInTimeZone(period.start, timeZone);
      const [sheepCouncilRaw, eapRaw, githubIssuesDistinctUserCount] =
        await Promise.all([
          latestRawMetric(
            prisma,
            weekStartDate,
            PlgWeeklyMetricRawType.SHEEP_COUNCIL_MEETINGS,
          ),
          latestRawMetric(
            prisma,
            weekStartDate,
            PlgWeeklyMetricRawType.EAP_MEETINGS,
          ),
          githubCount(period),
        ]);
      if (!sheepCouncilRaw) {
        throw new Error(
          `No Sheep Council Zoom capture exists for ${period.start}`,
        );
      }
      if (!eapRaw) {
        throw new Error(`No Luma EAP capture exists for ${period.start}`);
      }
      return {
        githubIssuesDistinctUserCount,
        lumaEapSignupCount: lumaEapSignupCount(eapRaw.rawMetricData),
        lumaEapZoomAttendanceCount: lumaEapAttendanceCount(
          eapRaw.rawMetricData,
        ),
        recordedAt,
        sheepCouncilActiveCount: null,
        sheepCouncilDiscordUserCount: null,
        sheepCouncilZoomAttendanceCount: sheepCouncilAttendanceCount(
          sheepCouncilRaw.rawMetricData,
        ),
        sourcesRecordedAt: {
          eap: eapRaw.recordedAt,
          sheepCouncil: sheepCouncilRaw.recordedAt,
        },
        totalDiscordUserCount: null,
        weekStartDate,
      };
    }),
  );

  console.log(
    JSON.stringify(
      {
        applied: apply,
        rows,
        window: {
          end: periods.at(-1)?.end,
          start: periods[0]?.start,
          type: 'six-most-recent-closed-weeks',
        },
      },
      null,
      2,
    ),
  );

  if (apply) {
    await prisma.$transaction(
      rows.map(({ sourcesRecordedAt: _sourcesRecordedAt, ...row }) => {
        const { weekStartDate, ...values } = row;
        return prisma.plgWeeklyMetric.upsert({
          create: row,
          update: values,
          where: { weekStartDate },
        });
      }),
    );
  }
} finally {
  await disconnectDatabase();
}
