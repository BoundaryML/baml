import {
  createZoomClient,
  type ZoomConfig,
  type ZoomMeetingInstance,
} from '../clients/zoom.js';
import type { RawMetricPeriod } from '../external-data.js';
import type { Prisma, PrismaClient } from '../generated/prisma/client.js';
import { PlgWeeklyMetricRawType } from '../generated/prisma/enums.js';
import { startOfDayInTimeZone } from '../snapshot.js';

export interface SheepCouncilZoomSyncConfig {
  meetingId: string;
  timeZone: string;
  zoom: ZoomConfig;
}

export interface SheepCouncilZoomSyncResult {
  instanceCount: number;
  recordedAt: Date;
  weekStartDate: Date;
}

function inputJson(value: unknown): Prisma.InputJsonValue {
  return JSON.parse(JSON.stringify(value)) as Prisma.InputJsonValue;
}

async function captureParticipants(
  zoom: Awaited<ReturnType<typeof createZoomClient>>,
  instance: ZoomMeetingInstance,
) {
  const [past, report] = await Promise.all([
    zoom.participantPages(instance.uuid, 'past'),
    zoom.participantPages(instance.uuid, 'report'),
  ]);
  return {
    instance: instance.raw,
    participants: { past, report },
  };
}

export async function fetchSheepCouncilZoomData(
  config: SheepCouncilZoomSyncConfig,
  prisma: PrismaClient,
  period: RawMetricPeriod,
  now = new Date(),
  fetchImpl: typeof fetch = fetch,
): Promise<SheepCouncilZoomSyncResult> {
  const weekStartDate = startOfDayInTimeZone(period.start, config.timeZone);
  const weekEndDate = startOfDayInTimeZone(period.end, config.timeZone);
  const zoom = await createZoomClient(config.zoom, fetchImpl);
  const instanceResponse = await zoom.meetingInstances(config.meetingId);
  const instances = instanceResponse.instances.filter((instance) => {
    const startTime = new Date(instance.startTime);
    return startTime >= weekStartDate && startTime < weekEndDate;
  });
  const captures = await Promise.all(
    instances.map((instance) => captureParticipants(zoom, instance)),
  );
  const rawMetricData = inputJson({
    meetingId: config.meetingId,
    period: {
      end: weekEndDate.toISOString(),
      start: weekStartDate.toISOString(),
      timeZone: config.timeZone,
    },
    source: 'sheep-council-zoom',
    version: 1,
    zoom: {
      captures,
      instancesEndpoint: instanceResponse.endpoint,
      instancesResponse: instanceResponse.response,
    },
  });
  await prisma.plgWeeklyMetricRaw.create({
    data: {
      rawMetricData,
      rawMetricType: PlgWeeklyMetricRawType.SHEEP_COUNCIL_MEETINGS,
      recordedAt: now,
      weekStartDate,
    },
  });
  return { instanceCount: captures.length, recordedAt: now, weekStartDate };
}
