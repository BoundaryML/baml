import type {
  LoopsCampaign,
  LoopsClient,
  UpdateLoopsEmailMessageInput,
} from './clients/loops.js';

export interface SheepCouncilEmailSpec {
  campaign: {
    campaignGroupId: string;
    mailingListId: string;
    name: string;
  };
  message: Omit<UpdateLoopsEmailMessageInput, 'expectedRevisionId'>;
}

export interface PrepareSheepCouncilEmailResult {
  action: 'create' | 'update';
  applied: boolean;
  campaignId: string | null;
  campaignUrl: string | null;
  spec: SheepCouncilEmailSpec;
}

type UnknownRecord = Record<string, unknown>;

function asRecord(value: unknown, name: string): UnknownRecord {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error(`Frontmatter ${name} must be an object`);
  }
  return value as UnknownRecord;
}

function requiredString(record: UnknownRecord, key: string): string {
  const value = record[key];
  if (typeof value !== 'string' || !value.trim()) {
    throw new Error(`frontmatter.${key} must be a non-empty string`);
  }
  return value.trim();
}

function stringRecord(value: unknown, name: string): Record<string, string> {
  const record = asRecord(value, name);
  for (const [key, item] of Object.entries(record)) {
    if (typeof item !== 'string') {
      throw new Error(`Frontmatter ${name}.${key} must be a string`);
    }
  }
  return record as Record<string, string>;
}

function parseFrontmatter(source: string): {
  body: string;
  frontmatter: UnknownRecord;
} {
  const lines = source.replaceAll('\r\n', '\n').split('\n');
  if (lines[0] !== '---') {
    throw new Error('Email draft must begin with frontmatter');
  }
  const end = lines.indexOf('---', 1);
  if (end < 0) throw new Error('Email draft frontmatter is not closed');
  const frontmatter: UnknownRecord = {};
  for (const line of lines.slice(1, end)) {
    if (!line.trim() || line.trimStart().startsWith('#')) continue;
    const match = /^([A-Za-z][A-Za-z0-9]*):\s*(.+)$/.exec(line);
    if (!match) throw new Error(`Unsupported frontmatter line: ${line}`);
    const [, key, rawValue] = match;
    if (!key || rawValue === undefined) {
      throw new Error(`Invalid frontmatter line: ${line}`);
    }
    if (key in frontmatter) {
      throw new Error(`Duplicate frontmatter key: ${key}`);
    }
    try {
      frontmatter[key] = JSON.parse(rawValue);
    } catch {
      frontmatter[key] = rawValue.trim();
    }
  }
  const body = lines
    .slice(end + 1)
    .join('\n')
    .trim();
  if (!body) throw new Error('Email LMX cannot be empty');
  return { body, frontmatter };
}

export function parseSheepCouncilEmailLmx(
  source: string,
): SheepCouncilEmailSpec {
  const { body: lmx, frontmatter } = parseFrontmatter(source);
  const emailFormat = requiredString(frontmatter, 'emailFormat');
  if (emailFormat !== 'styled' && emailFormat !== 'plain') {
    throw new Error('frontmatter.emailFormat must be styled or plain');
  }
  const fromEmail = requiredString(frontmatter, 'fromEmail');
  if (fromEmail.includes('@')) {
    throw new Error(
      'frontmatter.fromEmail must be the local part configured in Loops',
    );
  }
  if (emailFormat === 'styled' && !lmx.startsWith('<Style ')) {
    throw new Error('Styled email LMX must begin with a <Style /> element');
  }
  return {
    campaign: {
      campaignGroupId: requiredString(frontmatter, 'campaignGroupId'),
      mailingListId: requiredString(frontmatter, 'mailingListId'),
      name: requiredString(frontmatter, 'name'),
    },
    message: {
      contactPropertiesFallbacks: stringRecord(
        frontmatter.contactPropertiesFallbacks,
        'contactPropertiesFallbacks',
      ),
      emailFormat,
      fromEmail,
      fromName: requiredString(frontmatter, 'fromName'),
      lmx,
      previewText: requiredString(frontmatter, 'previewText'),
      replyToEmail: requiredString(frontmatter, 'replyToEmail'),
      subject: requiredString(frontmatter, 'subject'),
    },
  };
}

export function assertSheepCouncilDraftReady(source: string): void {
  if (/\bTODO\b/i.test(source)) {
    throw new Error(
      'The LMX email draft still contains TODO; finish editing it before using --apply',
    );
  }
}

function chooseCampaign(
  campaigns: LoopsCampaign[],
  campaignGroupId: string,
  campaignName: string,
): LoopsCampaign | undefined {
  const matches = campaigns.filter(
    (campaign) =>
      campaign.campaignGroupId === campaignGroupId &&
      campaign.name === campaignName,
  );
  const drafts = matches.filter((campaign) => campaign.status === 'Draft');
  if (drafts.length > 1) {
    throw new Error(
      `Found ${drafts.length} draft campaigns named ${JSON.stringify(campaignName)} in the Sheep Council group`,
    );
  }
  if (drafts.length === 1) return drafts[0];
  if (matches.length > 0) {
    throw new Error(
      `Campaign ${JSON.stringify(campaignName)} already exists with status ${matches.map((campaign) => campaign.status).join(', ')}`,
    );
  }
  return undefined;
}

export async function prepareSheepCouncilEmail(
  client: LoopsClient,
  spec: SheepCouncilEmailSpec,
  apply: boolean,
): Promise<PrepareSheepCouncilEmailResult> {
  const campaigns = await client.listCampaigns();
  const existing = chooseCampaign(
    campaigns,
    spec.campaign.campaignGroupId,
    spec.campaign.name,
  );
  const action = existing ? 'update' : 'create';
  if (!apply) {
    return {
      action,
      applied: false,
      campaignId: existing?.id ?? null,
      campaignUrl: existing?.url ?? null,
      spec,
    };
  }

  let campaign: LoopsCampaign;
  let revisionId: string | null;
  if (existing) {
    if (!existing.emailMessageId) {
      throw new Error(`Draft campaign ${existing.id} has no email message`);
    }
    const message = await client.getEmailMessage(existing.emailMessageId);
    revisionId = message.contentRevisionId;
    campaign = await client.updateCampaign(existing.id, {
      ...spec.campaign,
    });
  } else {
    const created = await client.createCampaign(spec.campaign);
    campaign = created;
    revisionId = created.emailMessageContentRevisionId;
  }
  if (!campaign.emailMessageId || !revisionId) {
    throw new Error(
      `Draft campaign ${campaign.id} has no editable email revision`,
    );
  }
  await client.updateEmailMessage(campaign.emailMessageId, {
    ...spec.message,
    expectedRevisionId: revisionId,
  });
  return {
    action,
    applied: true,
    campaignId: campaign.id,
    campaignUrl: campaign.url,
    spec,
  };
}
