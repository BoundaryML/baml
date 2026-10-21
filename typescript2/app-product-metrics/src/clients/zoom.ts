const zoomApiBase = 'https://api.zoom.us/v2';

export interface ZoomConfig {
  accountId: string;
  clientId: string;
  clientSecret: string;
}

export interface ZoomMeetingInstance {
  raw: Record<string, unknown>;
  startTime: string;
  uuid: string;
}

export interface ZoomMeetingInstancesCapture {
  endpoint: string;
  instances: ZoomMeetingInstance[];
  response: Record<string, unknown>;
}

export interface ZoomParticipantPagesCapture {
  endpoint: string;
  pages: Array<{
    request: string;
    response: Record<string, unknown>;
  }>;
}

export interface ZoomClient {
  meetingInstances: (meetingId: string) => Promise<ZoomMeetingInstancesCapture>;
  participantPages: (
    instanceUuid: string,
    kind: 'past' | 'report',
  ) => Promise<ZoomParticipantPagesCapture>;
}

function record(value: unknown, resource: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Zoom returned an invalid ${resource} response`);
  }
  return value as Record<string, unknown>;
}

function instances(value: Record<string, unknown>): ZoomMeetingInstance[] {
  if (!Array.isArray(value.meetings)) {
    throw new Error('Zoom returned an invalid meeting instances response');
  }
  return value.meetings.map((candidate) => {
    const raw = record(candidate, 'meeting instance');
    if (typeof raw.uuid !== 'string' || typeof raw.start_time !== 'string') {
      throw new Error('Zoom returned an invalid meeting instance');
    }
    const startTime = new Date(raw.start_time);
    if (!Number.isFinite(startTime.getTime())) {
      throw new Error('Zoom returned an invalid meeting instance start_time');
    }
    return { raw, startTime: raw.start_time, uuid: raw.uuid };
  });
}

function encodedUuid(uuid: string): string {
  return encodeURIComponent(encodeURIComponent(uuid));
}

export async function createZoomClient(
  config: ZoomConfig,
  fetchImpl: typeof fetch = fetch,
): Promise<ZoomClient> {
  const tokenResponse = await fetchImpl(
    `https://zoom.us/oauth/token?grant_type=account_credentials&account_id=${encodeURIComponent(config.accountId)}`,
    {
      headers: {
        Authorization: `Basic ${Buffer.from(`${config.clientId}:${config.clientSecret}`).toString('base64')}`,
      },
      method: 'POST',
      signal: AbortSignal.timeout(30_000),
    },
  );
  const tokenBody = record(await tokenResponse.json(), 'OAuth token');
  if (!tokenResponse.ok || typeof tokenBody.access_token !== 'string') {
    throw new Error(`Zoom token request failed: ${tokenResponse.status}`);
  }
  const accessToken = tokenBody.access_token;

  async function get(pathname: string): Promise<Record<string, unknown>> {
    const response = await fetchImpl(`${zoomApiBase}${pathname}`, {
      headers: { Authorization: `Bearer ${accessToken}` },
      signal: AbortSignal.timeout(30_000),
    });
    const body = record(
      await response.json(),
      pathname.split('?')[0] ?? pathname,
    );
    if (!response.ok) {
      throw new Error(
        `Zoom request failed for ${pathname.split('?')[0]}: ${response.status} ${String(body.message ?? body.code ?? '')}`,
      );
    }
    return body;
  }

  return {
    async meetingInstances(
      meetingId: string,
    ): Promise<ZoomMeetingInstancesCapture> {
      const endpoint = `/past_meetings/${encodeURIComponent(meetingId)}/instances`;
      const response = await get(endpoint);
      return { endpoint, instances: instances(response), response };
    },
    async participantPages(
      instanceUuid: string,
      kind: 'past' | 'report',
    ): Promise<ZoomParticipantPagesCapture> {
      const endpoint =
        kind === 'past'
          ? `/past_meetings/${encodedUuid(instanceUuid)}/participants`
          : `/report/meetings/${encodedUuid(instanceUuid)}/participants`;
      const pages: ZoomParticipantPagesCapture['pages'] = [];
      let nextPageToken = '';
      do {
        const parameters = new URLSearchParams({ page_size: '300' });
        if (nextPageToken) {
          parameters.set('next_page_token', nextPageToken);
        }
        const request = `${endpoint}?${parameters}`;
        const response = await get(request);
        pages.push({ request, response });
        nextPageToken =
          typeof response.next_page_token === 'string'
            ? response.next_page_token
            : '';
      } while (nextPageToken);
      return { endpoint, pages };
    },
  };
}
