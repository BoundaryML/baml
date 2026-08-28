export type RawMetricType = 'sheep-council-meetings' | 'eap-meetings';

export interface RawMetricPeriod {
  end: string;
  start: string;
}

export interface ExternalDataSyncRequest {
  raw_metric_period: RawMetricPeriod;
  raw_metric_type: RawMetricType;
}

function record(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
  return value as Record<string, unknown>;
}

function requireExactKeys(
  value: Record<string, unknown>,
  keys: string[],
  name: string,
): void {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error(`${name} must contain exactly ${expected.join(', ')}`);
  }
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

export function parseExternalDataSyncRequest(
  value: unknown,
): ExternalDataSyncRequest {
  const request = record(value, 'request body');
  requireExactKeys(
    request,
    ['raw_metric_period', 'raw_metric_type'],
    'request body',
  );
  if (
    request.raw_metric_type !== 'sheep-council-meetings' &&
    request.raw_metric_type !== 'eap-meetings'
  ) {
    throw new Error(
      'raw_metric_type must be sheep-council-meetings or eap-meetings',
    );
  }
  const period = record(request.raw_metric_period, 'raw_metric_period');
  requireExactKeys(period, ['end', 'start'], 'raw_metric_period');
  const start = isoDate(period.start, 'raw_metric_period.start');
  const end = isoDate(period.end, 'raw_metric_period.end');
  const dayMs = 24 * 60 * 60 * 1000;
  if (start.date.getUTCDay() !== 1 || end.date.getUTCDay() !== 1) {
    throw new Error('raw_metric_period must start and end on Mondays');
  }
  if (end.date.getTime() - start.date.getTime() !== 7 * dayMs) {
    throw new Error('raw_metric_period must span exactly one week');
  }
  return {
    raw_metric_period: { end: end.value, start: start.value },
    raw_metric_type: request.raw_metric_type,
  };
}
