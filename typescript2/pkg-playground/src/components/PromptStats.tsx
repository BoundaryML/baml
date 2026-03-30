import type { FC } from 'react';

interface PromptStatsProps {
  text: string;
}

export const PromptStats: FC<PromptStatsProps> = ({ text }) => {
  if (!text) return null;

  const chars = text.length;
  const words = text.split(/\s+/).filter(Boolean).length;
  const lines = text.split('\n').length;
  const estimatedTokens = Math.ceil(chars / 4);

  return (
    <div className="flex items-center gap-3 px-2.5 py-1 text-[10px] text-vsc-text-faint font-vsc-mono border-t border-vsc-border bg-vsc-bg-secondary shrink-0">
      <span>{chars.toLocaleString()} chars</span>
      <span>{words.toLocaleString()} words</span>
      <span>{lines} lines</span>
      <span>~{estimatedTokens.toLocaleString()} tokens</span>
    </div>
  );
};
