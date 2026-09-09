export interface LoopsConfig {
  apiKey: string;
  baseUrl?: string;
}

export interface LoopsCampaign {
  campaignGroupId: string | null;
  emailMessageId: string | null;
  id: string;
  mailingListId: string | null;
  name: string;
  status: 'Draft' | 'Scheduled' | 'Sending' | 'Sent';
  url: string;
}

export interface LoopsCampaignGroup {
  id: string;
  name: string;
}

export interface LoopsMailingList {
  id: string;
  name: string;
}

export interface LoopsEmailMessage {
  contentRevisionId: string | null;
  id: string;
  lmx: string;
  subject: string;
}

export interface CreateLoopsCampaignResult extends LoopsCampaign {
  emailMessageContentRevisionId: string | null;
}

export interface UpdateLoopsEmailMessageInput {
  contactPropertiesFallbacks?: Record<string, string>;
  emailFormat: 'plain' | 'styled';
  expectedRevisionId: string;
  fromEmail: string;
  fromName: string;
  languageCode?: string;
  lmx: string;
  previewText: string;
  replyToEmail: string;
  subject: string;
}

export interface LoopsClient {
  createCampaign(input: {
    campaignGroupId: string;
    mailingListId: string;
    name: string;
  }): Promise<CreateLoopsCampaignResult>;
  getEmailMessage(emailMessageId: string): Promise<LoopsEmailMessage>;
  listCampaignGroups(): Promise<LoopsCampaignGroup[]>;
  listCampaigns(): Promise<LoopsCampaign[]>;
  listMailingLists(): Promise<LoopsMailingList[]>;
  updateCampaign(
    campaignId: string,
    input: {
      campaignGroupId: string;
      mailingListId: string;
      name: string;
    },
  ): Promise<LoopsCampaign>;
  updateEmailMessage(
    emailMessageId: string,
    input: UpdateLoopsEmailMessageInput,
  ): Promise<LoopsEmailMessage>;
}

type UnknownRecord = Record<string, unknown>;

function asRecord(value: unknown, context: string): UnknownRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`Loops returned an invalid ${context}`);
  }
  return value as UnknownRecord;
}

function requiredString(
  record: UnknownRecord,
  key: string,
  context: string,
): string {
  const value = record[key];
  if (typeof value !== 'string') {
    throw new Error(`Loops returned an invalid ${context}.${key}`);
  }
  return value;
}

function nullableString(
  record: UnknownRecord,
  key: string,
  context: string,
): string | null {
  const value = record[key];
  if (typeof value !== 'string' && value !== null) {
    throw new Error(`Loops returned an invalid ${context}.${key}`);
  }
  return value;
}

function parseCampaign(value: unknown): LoopsCampaign {
  const campaign = asRecord(value, 'campaign');
  const status = requiredString(campaign, 'status', 'campaign');
  if (!['Draft', 'Scheduled', 'Sending', 'Sent'].includes(status)) {
    throw new Error(`Loops returned an unknown campaign status: ${status}`);
  }
  return {
    campaignGroupId: nullableString(campaign, 'campaignGroupId', 'campaign'),
    emailMessageId: nullableString(campaign, 'emailMessageId', 'campaign'),
    id: requiredString(campaign, 'id', 'campaign'),
    mailingListId: nullableString(campaign, 'mailingListId', 'campaign'),
    name: requiredString(campaign, 'name', 'campaign'),
    status: status as LoopsCampaign['status'],
    url: requiredString(campaign, 'url', 'campaign'),
  };
}

function parseEmailMessage(value: unknown): LoopsEmailMessage {
  const message = asRecord(value, 'email message');
  return {
    contentRevisionId: nullableString(
      message,
      'contentRevisionId',
      'email message',
    ),
    id: requiredString(message, 'id', 'email message'),
    lmx: requiredString(message, 'lmx', 'email message'),
    subject: requiredString(message, 'subject', 'email message'),
  };
}

function errorMessage(value: unknown): string | undefined {
  if (typeof value === 'string') return value;
  if (typeof value !== 'object' || value === null) return undefined;
  const record = value as UnknownRecord;
  for (const key of ['message', 'error', 'detail']) {
    if (typeof record[key] === 'string') return record[key];
  }
  return undefined;
}

export function createLoopsClient(
  config: LoopsConfig,
  fetchImpl: typeof fetch = fetch,
): LoopsClient {
  const baseUrl = (config.baseUrl ?? 'https://app.loops.so/api').replace(
    /\/$/,
    '',
  );

  async function request(path: string, init?: RequestInit): Promise<unknown> {
    const response = await fetchImpl(`${baseUrl}${path}`, {
      ...init,
      headers: {
        Authorization: `Bearer ${config.apiKey}`,
        'Content-Type': 'application/json; charset=utf-8',
        ...init?.headers,
      },
      signal: AbortSignal.timeout(30_000),
    });
    let result: unknown;
    try {
      result = await response.json();
    } catch {
      result = undefined;
    }
    if (!response.ok) {
      throw new Error(
        `Loops request ${init?.method ?? 'GET'} ${path} failed (${response.status})${errorMessage(result) ? `: ${errorMessage(result)}` : ''}`,
      );
    }
    return result;
  }

  async function paginated(path: string): Promise<unknown[]> {
    const values: unknown[] = [];
    let cursor: string | null = null;
    const seenCursors = new Set<string>();
    do {
      const query = new URLSearchParams({ perPage: '50' });
      if (cursor) query.set('cursor', cursor);
      const result = asRecord(
        await request(`${path}?${query.toString()}`),
        'paginated response',
      );
      if (!Array.isArray(result.data)) {
        throw new Error('Loops returned an invalid paginated response.data');
      }
      values.push(...result.data);
      const pagination = asRecord(result.pagination, 'pagination');
      const nextCursor = pagination.nextCursor;
      if (nextCursor !== null && typeof nextCursor !== 'string') {
        throw new Error('Loops returned an invalid pagination.nextCursor');
      }
      if (nextCursor && seenCursors.has(nextCursor)) {
        throw new Error('Loops returned a repeated pagination cursor');
      }
      if (nextCursor) seenCursors.add(nextCursor);
      cursor = nextCursor;
    } while (cursor);
    return values;
  }

  return {
    async createCampaign(input) {
      const result = asRecord(
        await request('/v1/campaigns', {
          body: JSON.stringify(input),
          method: 'POST',
        }),
        'created campaign',
      );
      return {
        ...parseCampaign(result),
        emailMessageContentRevisionId: nullableString(
          result,
          'emailMessageContentRevisionId',
          'created campaign',
        ),
      };
    },
    async getEmailMessage(emailMessageId) {
      return parseEmailMessage(
        await request(
          `/v1/email-messages/${encodeURIComponent(emailMessageId)}`,
        ),
      );
    },
    async listCampaignGroups() {
      const groups = await paginated('/v1/campaign-groups');
      return groups.map((value) => {
        const group = asRecord(value, 'campaign group');
        return {
          id: requiredString(group, 'id', 'campaign group'),
          name: requiredString(group, 'name', 'campaign group'),
        };
      });
    },
    async listCampaigns() {
      return (await paginated('/v1/campaigns')).map(parseCampaign);
    },
    async listMailingLists() {
      const result = await request('/v1/lists');
      if (!Array.isArray(result)) {
        throw new Error('Loops returned an invalid mailing list response');
      }
      return result.map((value) => {
        const list = asRecord(value, 'mailing list');
        return {
          id: requiredString(list, 'id', 'mailing list'),
          name: requiredString(list, 'name', 'mailing list'),
        };
      });
    },
    async updateCampaign(campaignId, input) {
      return parseCampaign(
        await request(`/v1/campaigns/${encodeURIComponent(campaignId)}`, {
          body: JSON.stringify(input),
          method: 'POST',
        }),
      );
    },
    async updateEmailMessage(emailMessageId, input) {
      return parseEmailMessage(
        await request(
          `/v1/email-messages/${encodeURIComponent(emailMessageId)}`,
          { body: JSON.stringify(input), method: 'POST' },
        ),
      );
    },
  };
}
