export interface PostHogConfig {
  host: string;
  personalApiKey: string;
  projectId: string;
}

interface HogQlResponse {
  columns?: unknown;
  results?: unknown;
}

export interface HogQlResult {
  columns: string[];
  results: unknown[][];
}

export async function runHogQl(
  config: PostHogConfig,
  name: string,
  query: string,
  fetchImpl: typeof fetch = fetch,
): Promise<HogQlResult> {
  const response = await fetchImpl(
    `${config.host.replace(/\/$/, '')}/api/projects/${encodeURIComponent(config.projectId)}/query/`,
    {
      body: JSON.stringify({ name, query: { kind: 'HogQLQuery', query } }),
      headers: {
        Authorization: `Bearer ${config.personalApiKey}`,
        'Content-Type': 'application/json; charset=utf-8',
      },
      method: 'POST',
      signal: AbortSignal.timeout(30_000),
    },
  );
  const result = (await response.json()) as HogQlResponse & { detail?: string };
  if (!response.ok) {
    throw new Error(
      `PostHog query failed: ${result.detail ?? response.status}`,
    );
  }
  if (
    !Array.isArray(result.columns) ||
    !result.columns.every((column) => typeof column === 'string') ||
    !Array.isArray(result.results) ||
    !result.results.every((row) => Array.isArray(row))
  ) {
    throw new Error('PostHog query returned an unexpected result shape');
  }
  return { columns: result.columns, results: result.results };
}
