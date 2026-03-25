import type { FC } from 'react';
import type { FetchLogEntry } from '../worker-protocol';
import { Clock, Cpu, Hash } from 'lucide-react';

interface MetadataBadgesProps {
  fetchLogs: FetchLogEntry[];
  durationMs?: number | null;
}

function extractMetadata(logs: FetchLogEntry[]) {
  let model: string | null = null;
  let totalTokens: number | null = null;
  let processingMs: number | null = null;

  for (const log of logs) {
    // Try response headers first (works in native/Rust transport where CORS doesn't apply)
    const h = log.responseHeaders;
    if (h) {
      if (!model && h['openai-model']) model = h['openai-model'];
      if (!processingMs && h['openai-processing-ms']) processingMs = parseInt(h['openai-processing-ms'], 10);
      if (!model && h['x-model']) model = h['x-model'];
      const tokenHeader = h['x-total-tokens'] || h['x-ratelimit-remaining-tokens'];
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
          else if (typeof u.input_tokens === 'number' && typeof u.output_tokens === 'number') {
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

const badgeCls = 'inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-vsc-mono bg-vsc-bg-secondary text-vsc-text-muted border border-vsc-border';

export const MetadataBadges: FC<MetadataBadgesProps> = ({ fetchLogs, durationMs }) => {
  const { model, totalTokens, processingMs } = extractMetadata(fetchLogs);
  const latency = processingMs ?? (durationMs ? Math.round(durationMs) : null);

  if (!model && !totalTokens && !latency) return null;

  return (
    <div className="flex items-center gap-1.5 flex-wrap">
      {model && <span className={badgeCls}><Cpu size={10} />{model}</span>}
      {latency != null && <span className={badgeCls}><Clock size={10} />{latency}ms</span>}
      {totalTokens != null && <span className={badgeCls}><Hash size={10} />{totalTokens} tokens</span>}
    </div>
  );
};
