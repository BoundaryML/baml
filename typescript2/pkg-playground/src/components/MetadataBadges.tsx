import type { FC } from 'react';
import { Clock, Cpu, Hash } from 'lucide-react';
import { Badge } from './ui/badge';

export interface MetadataFetchLog {
  responseHeaders: Record<string, string> | null;
  responseBody: string | null;
}

interface MetadataBadgesProps {
  fetchLogs: MetadataFetchLog[];
  durationMs?: number | null;
}

function extractMetadata(logs: MetadataFetchLog[]) {
  let model: string | null = null;
  let totalTokens: number | null = null;
  let processingMs: number | null = null;

  for (const log of logs) {
    // Try response headers first (works in native/Rust transport where CORS doesn't apply)
    const h = log.responseHeaders;
    if (h) {
      if (!model && h['openai-model']) model = h['openai-model'];
      if (!processingMs && h['openai-processing-ms'])
        processingMs = parseInt(h['openai-processing-ms'], 10);
      if (!model && h['x-model']) model = h['x-model'];
      const tokenHeader =
        h['x-total-tokens'] || h['x-ratelimit-remaining-tokens'];
      if (!totalTokens && tokenHeader) totalTokens = parseInt(tokenHeader, 10);
    }

    // Parse response body for metadata (works in browser/WASM where CORS strips custom headers)
    if (log.responseBody && (!model || totalTokens == null)) {
      try {
        const body = JSON.parse(log.responseBody);
        if (!model && typeof body.model === 'string') model = body.model;
        if (totalTokens == null && body.usage) {
          const u = body.usage;
          if (typeof u.total_tokens === 'number') totalTokens = u.total_tokens;
          else if (
            typeof u.input_tokens === 'number' &&
            typeof u.output_tokens === 'number'
          ) {
            totalTokens = u.input_tokens + u.output_tokens;
          }
        }
      } catch {
        // Not JSON or doesn't have expected fields — skip
      }
    }
  }

  return { model, totalTokens, processingMs };
}

export const MetadataBadges: FC<MetadataBadgesProps> = ({
  fetchLogs,
  durationMs,
}) => {
  const { model, totalTokens, processingMs } = extractMetadata(fetchLogs);
  const latency = processingMs ?? (durationMs ? Math.round(durationMs) : null);

  if (!model && !totalTokens && !latency) return null;

  return (
    <div className="flex items-center gap-1.5 flex-wrap">
      {model && (
        <Badge
          variant="secondary"
          className="gap-0.5 text-[10px] font-vsc-mono"
        >
          <Cpu size={10} />
          {model}
        </Badge>
      )}
      {latency != null && (
        <Badge
          variant="secondary"
          className="gap-0.5 text-[10px] font-vsc-mono"
        >
          <Clock size={10} />
          {latency}ms
        </Badge>
      )}
      {totalTokens != null && (
        <Badge
          variant="secondary"
          className="gap-0.5 text-[10px] font-vsc-mono"
        >
          <Hash size={10} />
          {totalTokens} tokens
        </Badge>
      )}
    </div>
  );
};
