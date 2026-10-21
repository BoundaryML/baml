const discordApiBase = 'https://discord.com/api/v10';

export interface DiscordCommunityMetrics {
  approximateMemberCount: number;
  approximatePresenceCount: number;
  guildId: string;
  guildName: string;
  observedAt: string;
  sheepCouncilMemberCount: number;
  totalMemberCount: number;
}

export interface DiscordCommunityConfig {
  botToken: string;
  expectedGuildName: string;
  guildId: string;
  inviteCode: string;
  sheepCouncilRoleId: string;
}

function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('Discord returned an invalid invite response');
  }
  return value as Record<string, unknown>;
}

function stringField(value: Record<string, unknown>, field: string): string {
  const result = value[field];
  if (typeof result !== 'string' || !result) {
    throw new Error(`Discord returned an invalid ${field}`);
  }
  return result;
}

function countField(value: Record<string, unknown>, field: string): number {
  const result = value[field];
  if (!Number.isSafeInteger(result) || Number(result) < 0) {
    throw new Error(`Discord returned an invalid ${field}`);
  }
  return Number(result);
}

function records(value: unknown): Record<string, unknown>[] {
  if (!Array.isArray(value)) {
    throw new Error('Discord returned an invalid guild member response');
  }
  return value.map(record);
}

async function loadGuildMemberCounts(
  config: DiscordCommunityConfig,
  fetchImpl: typeof fetch,
): Promise<{ sheepCouncilMemberCount: number; totalMemberCount: number }> {
  let after = '0';
  let sheepCouncilMemberCount = 0;
  let totalMemberCount = 0;
  while (true) {
    const url = `${discordApiBase}/guilds/${encodeURIComponent(config.guildId)}/members?limit=1000&after=${encodeURIComponent(after)}`;
    const response = await fetchImpl(url, {
      headers: {
        Accept: 'application/json',
        Authorization: `Bot ${config.botToken}`,
        'User-Agent': 'Boundary product metrics (https://boundaryml.com, 1.0)',
      },
    });
    if (!response.ok) {
      throw new Error(`Discord guild member query failed: ${response.status}`);
    }
    const members = records(await response.json());
    totalMemberCount += members.length;
    for (const member of members) {
      const roles = member.roles;
      if (
        !Array.isArray(roles) ||
        !roles.every((role) => typeof role === 'string')
      ) {
        throw new Error('Discord returned invalid guild member roles');
      }
      if (roles.includes(config.sheepCouncilRoleId)) {
        sheepCouncilMemberCount += 1;
      }
    }
    if (members.length < 1000) {
      return { sheepCouncilMemberCount, totalMemberCount };
    }
    const lastUser = record(members.at(-1)?.user);
    const nextAfter = stringField(lastUser, 'id');
    if (nextAfter === after) {
      throw new Error('Discord guild member pagination did not advance');
    }
    after = nextAfter;
  }
}

export async function loadDiscordCommunityMetrics(
  config: DiscordCommunityConfig,
  fetchImpl: typeof fetch = fetch,
  now: Date = new Date(),
): Promise<DiscordCommunityMetrics> {
  const url = `${discordApiBase}/invites/${encodeURIComponent(config.inviteCode)}?with_counts=true`;
  const response = await fetchImpl(url, {
    headers: {
      Accept: 'application/json',
      'User-Agent': 'Boundary product metrics (https://boundaryml.com, 1.0)',
    },
  });
  if (!response.ok) {
    throw new Error(`Discord invite query failed: ${response.status}`);
  }
  const invite = record(await response.json());
  const guild = record(invite.guild);
  const guildName = stringField(guild, 'name');
  if (guildName !== config.expectedGuildName) {
    throw new Error(
      `Discord invite resolved to unexpected guild: ${guildName}`,
    );
  }
  const guildId = stringField(guild, 'id');
  if (guildId !== config.guildId) {
    throw new Error(
      `Discord invite resolved to unexpected guild ID: ${guildId}`,
    );
  }
  const memberCounts = await loadGuildMemberCounts(config, fetchImpl);
  return {
    approximateMemberCount: countField(invite, 'approximate_member_count'),
    approximatePresenceCount: countField(invite, 'approximate_presence_count'),
    guildId,
    guildName,
    observedAt: now.toISOString(),
    ...memberCounts,
  };
}
