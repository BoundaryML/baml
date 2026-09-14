import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Prisma, PrismaClient } from './generated/prisma/client.js';
import {
  aggregateWeeklyMetric,
  githubIssuesDistinctUsersQuery,
  parseWeeklyMetricPeriod,
} from './jobs/aggregate-weekly-metric.js';

function jsonResponse(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    headers: { 'Content-Type': 'application/json' },
    status,
  });
}

function participant(name: string, duration: number, internalUser = false) {
  return {
    duration,
    id: '',
    internal_user: internalUser,
    name,
    user_email: '',
  };
}

function participantCapture(
  uuid: string,
  participants: ReturnType<typeof participant>[],
) {
  return {
    instance: { uuid },
    participants: {
      past: { pages: [{ response: { participants } }] },
    },
  };
}

const sheepCouncilRaw = {
  zoom: {
    captures: [
      participantCapture('sheep-occurrence', [
        participant('Alice', 350),
        participant('alice', 300),
        participant('Bob', 599),
        participant('Boundary staff', 1_000, true),
      ]),
    ],
  },
};

const eapRaw = {
  captures: [
    {
      event: { id: 'eap-event' },
      guests: [
        {
          response: {
            entries: [
              { approval_status: 'approved', id: 'guest-1' },
              { approval_status: 'approved', id: 'guest-2' },
              { approval_status: 'approved', id: 'guest-1' },
            ],
          },
        },
      ],
      zoom: {
        ...participantCapture('eap-occurrence', [
          participant('EAP attendee', 600),
          participant('Boundary staff', 2_000, true),
        ]),
        resolution: 'resolved',
      },
    },
    {
      event: { id: 'unresolved-event' },
      guests: [{ response: { entries: [] } }],
      zoom: { resolution: 'not-found' },
    },
  ],
};

function fakePrisma() {
  const writes: Prisma.PlgWeeklyMetricUpsertArgs[] = [];
  const prisma = {
    plgWeeklyMetric: {
      async upsert(args: Prisma.PlgWeeklyMetricUpsertArgs) {
        writes.push(args);
        return args.create;
      },
    },
    plgWeeklyMetricRaw: {
      async findFirst(args: Prisma.PlgWeeklyMetricRawFindFirstArgs) {
        return {
          rawMetricData:
            args.where?.rawMetricType === 'SHEEP_COUNCIL_MEETINGS'
              ? sheepCouncilRaw
              : eapRaw,
        };
      },
    },
  } as unknown as PrismaClient;
  return { prisma, writes };
}

const config = {
  discord: {
    botToken: 'discord-token',
    expectedGuildName: 'Baml (by Boundary)',
    guildId: 'guild-id',
    inviteCode: 'invite',
    sheepCouncilRoleId: 'sheep-role',
  },
  posthog: {
    host: 'https://posthog.example.com',
    personalApiKey: 'posthog-key',
    projectId: 'project-id',
  },
  timeZone: 'America/Los_Angeles',
};

function successfulFetch(): typeof fetch {
  return async (input) => {
    const url = String(input);
    if (url.includes('/invites/')) {
      return jsonResponse({
        approximate_member_count: 3,
        approximate_presence_count: 2,
        guild: { id: 'guild-id', name: 'Baml (by Boundary)' },
      });
    }
    if (url.includes('/guilds/guild-id/members')) {
      return jsonResponse([
        { roles: ['sheep-role'] },
        { roles: [] },
        { roles: ['sheep-role'] },
      ]);
    }
    if (url.includes('/query/')) {
      return jsonResponse({ columns: ['distinct_users'], results: [[4]] });
    }
    throw new Error(`Unexpected request: ${url}`);
  };
}

test('parseWeeklyMetricPeriod requires an exact Monday-to-Monday period', () => {
  assert.deepEqual(
    parseWeeklyMetricPeriod({ end: '2026-08-24', start: '2026-08-17' }),
    { end: '2026-08-24', start: '2026-08-17' },
  );
  assert.throws(
    () => parseWeeklyMetricPeriod({ end: '2026-08-25', start: '2026-08-18' }),
    /Mondays/,
  );
  assert.throws(
    () =>
      parseWeeklyMetricPeriod({
        end: '2026-08-24',
        extra: true,
        start: '2026-08-17',
      }),
    /exactly/,
  );
});

test('githubIssuesDistinctUsersQuery targets the live BoundaryML/baml issues table', () => {
  const query = githubIssuesDistinctUsersQuery(
    { end: '2026-08-24', start: '2026-08-17' },
    'America/Los_Angeles',
  );
  assert.match(query, /FROM github_boundaryml_baml__issues/);
  assert.match(query, /JSONExtractString\(user, 'id'\)/);
  assert.match(query, /2026-08-17 00:00:00/);
  assert.match(query, /2026-08-24 00:00:00/);
});

test('aggregateWeeklyMetric collects every input before upserting one row', async () => {
  const { prisma, writes } = fakePrisma();
  const now = new Date('2026-08-27T20:00:00Z');
  const result = await aggregateWeeklyMetric(
    config,
    prisma,
    { end: '2026-08-24', start: '2026-08-17' },
    now,
    successfulFetch(),
  );

  assert.equal(writes.length, 1);
  assert.deepEqual(result, {
    githubIssuesDistinctUserCount: 4,
    lumaEapSignupCount: 2,
    lumaEapZoomAttendanceCount: 1,
    recordedAt: now,
    sheepCouncilActiveCount: null,
    sheepCouncilDiscordUserCount: 2,
    sheepCouncilZoomAttendanceCount: 1,
    totalDiscordUserCount: 3,
    weekStartDate: new Date('2026-08-17T07:00:00.000Z'),
  });
  assert.deepEqual(writes[0]?.create, result);
  assert.deepEqual(writes[0]?.where, {
    weekStartDate: new Date('2026-08-17T07:00:00.000Z'),
  });
  assert.deepEqual(writes[0]?.update, {
    githubIssuesDistinctUserCount: 4,
    lumaEapSignupCount: 2,
    lumaEapZoomAttendanceCount: 1,
    recordedAt: now,
    sheepCouncilActiveCount: null,
    sheepCouncilDiscordUserCount: 2,
    sheepCouncilZoomAttendanceCount: 1,
    totalDiscordUserCount: 3,
  });
});

test('aggregateWeeklyMetric does not upsert when a source operation fails', async () => {
  const { prisma, writes } = fakePrisma();
  const fetchImpl = successfulFetch();
  await assert.rejects(
    aggregateWeeklyMetric(
      config,
      prisma,
      { end: '2026-08-24', start: '2026-08-17' },
      new Date('2026-08-27T20:00:00Z'),
      async (input, init) => {
        if (String(input).includes('/query/')) {
          return jsonResponse({ detail: 'query failed' }, 500);
        }
        return fetchImpl(input, init);
      },
    ),
    /PostHog query failed/,
  );
  assert.equal(writes.length, 0);
});
