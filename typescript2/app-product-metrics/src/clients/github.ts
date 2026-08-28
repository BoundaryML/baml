const githubApiBase = 'https://api.github.com';

export interface GithubIssuesConfig {
  owner: string;
  repository: string;
  token?: string;
}

function records(value: unknown): Record<string, unknown>[] {
  if (!Array.isArray(value)) {
    throw new Error('GitHub returned an invalid issues response');
  }
  return value.map((entry) => {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new Error('GitHub returned an invalid issue');
    }
    return entry as Record<string, unknown>;
  });
}

function issueCreatedAt(issue: Record<string, unknown>): string {
  if (typeof issue.created_at !== 'string') {
    throw new Error('GitHub returned an issue without created_at');
  }
  return issue.created_at;
}

function issueAuthorId(issue: Record<string, unknown>): string | undefined {
  if (issue.user === null) return undefined;
  if (
    !issue.user ||
    typeof issue.user !== 'object' ||
    Array.isArray(issue.user)
  ) {
    throw new Error('GitHub returned an issue with an invalid user');
  }
  const id = (issue.user as Record<string, unknown>).id;
  if (typeof id !== 'number' && typeof id !== 'string') {
    throw new Error('GitHub returned an issue user without an id');
  }
  return String(id);
}

export async function loadDistinctGithubIssueAuthors(
  config: GithubIssuesConfig,
  periodStart: Date,
  periodEnd: Date,
  fetchImpl: typeof fetch = fetch,
): Promise<number> {
  const authorIds = new Set<string>();
  for (let page = 1; ; page += 1) {
    const parameters = new URLSearchParams({
      direction: 'desc',
      page: String(page),
      per_page: '100',
      sort: 'created',
      state: 'all',
    });
    const response = await fetchImpl(
      `${githubApiBase}/repos/${encodeURIComponent(config.owner)}/${encodeURIComponent(config.repository)}/issues?${parameters}`,
      {
        headers: {
          Accept: 'application/vnd.github+json',
          ...(config.token ? { Authorization: `Bearer ${config.token}` } : {}),
          'User-Agent': 'Boundary product metrics',
          'X-GitHub-Api-Version': '2022-11-28',
        },
        signal: AbortSignal.timeout(30_000),
      },
    );
    if (!response.ok) {
      throw new Error(`GitHub issues query failed: ${response.status}`);
    }
    const issues = records(await response.json());
    for (const issue of issues) {
      const createdAt = new Date(issueCreatedAt(issue));
      if (!Number.isFinite(createdAt.getTime())) {
        throw new Error('GitHub returned an invalid issue created_at');
      }
      if (
        !('pull_request' in issue) &&
        createdAt >= periodStart &&
        createdAt < periodEnd
      ) {
        const authorId = issueAuthorId(issue);
        if (authorId) authorIds.add(authorId);
      }
    }
    if (
      issues.length < 100 ||
      issues.some((issue) => new Date(issueCreatedAt(issue)) < periodStart)
    ) {
      return authorIds.size;
    }
  }
}
