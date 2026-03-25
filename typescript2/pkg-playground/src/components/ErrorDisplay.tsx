import { type FC, type ReactNode } from 'react';
import { CopyButton } from './CopyButton';
import { RefreshCw } from 'lucide-react';

interface ErrorContext {
  message: string;
  onRetry?: () => void;
}

interface CustomErrorRenderer {
  test: (message: string) => boolean;
  render: (context: ErrorContext) => ReactNode;
  priority: number;
}

const errorRenderers: CustomErrorRenderer[] = [];

export function registerErrorRenderer(renderer: CustomErrorRenderer) {
  errorRenderers.push(renderer);
  errorRenderers.sort((a, b) => b.priority - a.priority);
}

// Built-in renderers
registerErrorRenderer({
  priority: 150,
  test: (msg) => msg.includes('WASM panic') || msg.includes('unreachable executed') || msg.includes('RuntimeError: unreachable'),
  render: ({ message, onRetry }) => (
    <div className="space-y-2">
      <div className="text-[11px] font-semibold text-vsc-error">Internal Error (WASM)</div>
      <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border max-h-[120px] overflow-auto">
        {message.replace(/^.*WASM panic:\s*/, '')}
      </pre>
      <div className="text-[10px] text-vsc-text-faint">Try reopening the playground if this persists.</div>
      {onRetry && (
        <button onClick={onRetry} className="flex items-center gap-1 text-[10px] text-vsc-link hover:underline">
          <RefreshCw size={10} /> Retry
        </button>
      )}
    </div>
  ),
});

registerErrorRenderer({
  priority: 100,
  test: (msg) => msg.startsWith('Failed to fetch media') || (msg.includes('Failed to fetch') && msg.includes('http')),
  render: ({ message, onRetry }) => {
    const urlMatch = message.match(/https?:\/\/\S+/);
    return (
      <div className="space-y-2">
        <div className="text-[11px] font-semibold text-vsc-error">Network Error</div>
        <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border">
          {message}
        </pre>
        {urlMatch && <div className="text-[10px] text-vsc-text-faint">URL: {urlMatch[0]}</div>}
        {onRetry && (
          <button onClick={onRetry} className="flex items-center gap-1 text-[10px] text-vsc-link hover:underline">
            <RefreshCw size={10} /> Retry
          </button>
        )}
      </div>
    );
  },
});

registerErrorRenderer({
  priority: 0,
  test: () => true,
  render: ({ message, onRetry }) => (
    <div className="space-y-2">
      <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border max-h-[200px] overflow-auto">
        {message}
      </pre>
      {onRetry && (
        <button onClick={onRetry} className="flex items-center gap-1 text-[10px] text-vsc-link hover:underline">
          <RefreshCw size={10} /> Retry
        </button>
      )}
    </div>
  ),
});

interface ErrorDisplayProps {
  error: string;
  onRetry?: () => void;
}

export const ErrorDisplay: FC<ErrorDisplayProps> = ({ error, onRetry }) => {
  const renderer = errorRenderers.find((r) => r.test(error));
  if (!renderer) return <pre className="text-vsc-error text-[11px]">{error}</pre>;
  return (
    <div className="group relative">
      {renderer.render({ message: error, onRetry })}
      <div className="absolute top-1 right-1">
        <CopyButton text={error} />
      </div>
    </div>
  );
};
